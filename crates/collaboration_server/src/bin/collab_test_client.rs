use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use collaboration_server::domain::{decode_sync_message, encode_update, SyncMessage, MSG_SYNC};
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

    // Step 2: Receive SyncStep1 + SyncStep2 from server
    let doc = Doc::new();
    let mut received_step2 = false;

    for i in 0..2 {
        let msg = stream
            .next()
            .await
            .context("connection closed before initial sync")?
            .context("error receiving sync message")?;

        match msg {
            Message::Binary(data) => {
                if data.len() < 2 || data[0] != MSG_SYNC {
                    bail!("unexpected binary message during sync (message {})", i);
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
                    Ok(Some(SyncMessage::Update(_))) => {
                        bail!("unexpected Update message during init sync");
                    }
                    Ok(None) => {} // Non-sync message, ignore
                    Err(e) => bail!("failed to decode sync message during init: {}", e),
                }
            }
            Message::Close(frame) => {
                let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                bail!("server closed connection during sync: {}", reason);
            }
            _ => bail!("unexpected message type during sync"),
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
    }

    // Graceful close
    let _ = sink.send(Message::Close(None)).await;
    Ok(())
}
