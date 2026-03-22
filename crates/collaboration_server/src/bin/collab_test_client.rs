use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use collaboration_server::domain::{
    decode_sync_message, encode_update, SyncMessage, MSG_AWARENESS, MSG_SYNC,
};
use common::yrs::ROOT_TEXT_NAME;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, Text, Transact, Update};

#[derive(Parser)]
#[command(name = "collab-test-client")]
#[command(about = "Test client for the Ponix collaboration WebSocket server")]
struct Cli {
    /// WebSocket URL (e.g., ws://localhost:50052/ws/documents/DOC_ID)
    #[arg(long)]
    url: String,

    /// JWT token for authentication
    #[arg(long)]
    token: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Connect, insert text into the document, then disconnect
    Edit {
        /// Text to insert into the document
        #[arg(long)]
        text: String,
    },
    /// Connect, read the current document text, print to stdout, then disconnect
    Read,
    /// Connect, wait to receive updates from other clients, print received text
    Listen {
        /// How long to listen in seconds
        #[arg(long, default_value = "5")]
        duration: u64,
    },
    /// Connect and print presence/awareness info as JSON lines to stdout
    Presence {
        /// How long to listen for awareness messages in seconds
        #[arg(long, default_value = "5")]
        duration: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let (ws_stream, _) = tokio_tungstenite::connect_async(&cli.url)
        .await
        .context("failed to connect to WebSocket")?;

    let (mut sink, mut stream) = ws_stream.split();

    // Step 1: Send JWT as first text message
    sink.send(Message::Text(cli.token.into()))
        .await
        .context("failed to send auth token")?;

    // Step 2: Receive initial messages (SyncStep1 + SyncStep2 + awareness)
    let doc = Doc::new();
    let mut received_step2 = false;
    let mut awareness_messages: Vec<Vec<u8>> = Vec::new();

    // Read up to 5 messages within a timeout to handle sync + awareness
    for _ in 0..5 {
        let msg = match tokio::time::timeout(std::time::Duration::from_secs(3), stream.next()).await
        {
            Ok(Some(Ok(msg))) => msg,
            Ok(Some(Err(e))) => bail!("error receiving message: {}", e),
            Ok(None) => bail!("connection closed before initial sync"),
            Err(_) => break, // Timeout — done receiving init messages
        };

        match msg {
            Message::Binary(data) => {
                if data.is_empty() {
                    continue;
                }
                if data[0] == MSG_AWARENESS {
                    awareness_messages.push(data.to_vec());
                    continue;
                }
                if data[0] != MSG_SYNC {
                    continue;
                }
                match decode_sync_message(&data) {
                    Ok(Some(SyncMessage::SyncStep1(_))) => {
                        // Server's state vector — for a fresh client this is a no-op
                    }
                    Ok(Some(SyncMessage::SyncStep2(update_bytes))) => {
                        let update = Update::decode_v1(&update_bytes)
                            .context("failed to decode SyncStep2")?;
                        let mut txn = doc.transact_mut();
                        txn.apply_update(update)
                            .context("failed to apply SyncStep2")?;
                        received_step2 = true;
                    }
                    Ok(Some(SyncMessage::Update(_))) => {}
                    Ok(None) => {}
                    Err(e) => bail!("failed to decode sync message during init: {}", e),
                }
                // Once we have both step1 and step2, check if we should stop
                if received_step2 && awareness_messages.len() >= 1 {
                    break;
                }
            }
            Message::Close(frame) => {
                let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                bail!("server closed connection during sync: {}", reason);
            }
            _ => {}
        }
    }

    if !received_step2 {
        bail!("did not receive SyncStep2 from server");
    }

    match cli.command {
        Command::Edit { text } => {
            // Insert text into the root text type
            let update = {
                let content = doc.get_or_insert_text(ROOT_TEXT_NAME);
                let mut txn = doc.transact_mut();
                let len = content.get_string(&txn).len() as u32;
                content.insert(&mut txn, len, &text);
                txn.encode_update_v1()
            };

            // Send as sync Update message using the real protocol encoder
            let msg = encode_update(&update);
            sink.send(Message::Binary(msg.into()))
                .await
                .context("failed to send update")?;

            // Brief pause to ensure the server processes before we disconnect
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Print resulting text for verification
            let content = doc.get_or_insert_text(ROOT_TEXT_NAME);
            let txn = doc.transact();
            println!("{}", content.get_string(&txn));
        }
        Command::Read => {
            let content = doc.get_or_insert_text(ROOT_TEXT_NAME);
            let txn = doc.transact();
            println!("{}", content.get_string(&txn));
        }
        Command::Listen { duration } => {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(duration);

            loop {
                tokio::select! {
                    _ = tokio::time::sleep_until(deadline) => break,
                    msg = stream.next() => {
                        match msg {
                            Some(Ok(Message::Binary(data))) => {
                                if let Ok(Some(sync_msg)) = decode_sync_message(&data) {
                                    match sync_msg {
                                        SyncMessage::SyncStep2(bytes) | SyncMessage::Update(bytes) => {
                                            if let Ok(update) = Update::decode_v1(&bytes) {
                                                let mut txn = doc.transact_mut();
                                                let _ = txn.apply_update(update);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            _ => {}
                        }
                    }
                }
            }

            let content = doc.get_or_insert_text(ROOT_TEXT_NAME);
            let txn = doc.transact();
            println!("{}", content.get_string(&txn));
        }
        Command::Presence { duration } => {
            // Print awareness messages received during init
            for awareness_data in &awareness_messages {
                if let Some(json) = decode_awareness_json(awareness_data) {
                    println!("{}", json);
                }
            }

            // Listen for more awareness messages
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(duration);

            loop {
                tokio::select! {
                    _ = tokio::time::sleep_until(deadline) => break,
                    msg = stream.next() => {
                        match msg {
                            Some(Ok(Message::Binary(data))) => {
                                if !data.is_empty() && data[0] == MSG_AWARENESS {
                                    if let Some(json) = decode_awareness_json(&data) {
                                        println!("{}", json);
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // Graceful close
    let _ = sink.send(Message::Close(None)).await;
    Ok(())
}

/// Decode awareness wire format (Yjs lib0 varint encoding) and return the JSON presence entries.
/// Wire format: [MSG_AWARENESS][count: varint]([clientID: varint][clock: varint][state: var_string])*
fn decode_awareness_json(data: &[u8]) -> Option<String> {
    // Skip MSG_AWARENESS byte
    let data = &data[1..];

    let (count, mut pos) = read_var_uint(data, 0)?;
    let mut entries = Vec::new();

    for _ in 0..count {
        let (_client_id, new_pos) = read_var_uint(data, pos)?;
        let (_clock, new_pos) = read_var_uint(data, new_pos)?;
        let (str_len, new_pos) = read_var_uint(data, new_pos)?;
        let str_end = new_pos + str_len as usize;
        if str_end > data.len() {
            break;
        }
        if str_len > 0 {
            if let Ok(json_str) = std::str::from_utf8(&data[new_pos..str_end]) {
                if json_str != "null" {
                    entries.push(json_str.to_string());
                }
            }
        }
        pos = str_end;
    }

    if entries.is_empty() {
        None
    } else {
        Some(format!("[{}]", entries.join(",")))
    }
}

fn read_var_uint(data: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        if pos >= data.len() {
            return None;
        }
        let byte = data[pos];
        pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    Some((result, pos))
}
