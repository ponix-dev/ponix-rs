/// Top-level message types (first byte of a WebSocket binary message)
pub const MSG_SYNC: u8 = 0;
pub const MSG_AWARENESS: u8 = 1; // Reserved for #178

/// Sync sub-types (second byte, when first byte = MSG_SYNC)
pub const MSG_SYNC_STEP1: u8 = 0;
pub const MSG_SYNC_STEP2: u8 = 1;
pub const MSG_SYNC_UPDATE: u8 = 2;

#[derive(Debug, Clone, PartialEq)]
pub enum SyncMessage {
    SyncStep1(Vec<u8>),
    SyncStep2(Vec<u8>),
    Update(Vec<u8>),
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("empty message")]
    EmptyMessage,
    #[error("message too short: need at least 2 bytes for sync messages")]
    TooShort,
    #[error("unknown sync sub-type: {0}")]
    UnknownSyncSubType(u8),
}

impl SyncMessage {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            SyncMessage::SyncStep1(data) => encode_sync_step1(data),
            SyncMessage::SyncStep2(data) => encode_sync_step2(data),
            SyncMessage::Update(data) => encode_update(data),
        }
    }
}

/// Encode a SyncStep1 message containing a state vector
pub fn encode_sync_step1(state_vector: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(2 + state_vector.len());
    buf.push(MSG_SYNC);
    buf.push(MSG_SYNC_STEP1);
    buf.extend_from_slice(state_vector);
    buf
}

/// Encode a SyncStep2 message containing an update/diff
pub fn encode_sync_step2(update: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(2 + update.len());
    buf.push(MSG_SYNC);
    buf.push(MSG_SYNC_STEP2);
    buf.extend_from_slice(update);
    buf
}

/// Encode an Update message
pub fn encode_update(update: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(2 + update.len());
    buf.push(MSG_SYNC);
    buf.push(MSG_SYNC_UPDATE);
    buf.extend_from_slice(update);
    buf
}

/// Encode an awareness message (MSG_AWARENESS prefix + data)
pub fn encode_awareness_message(data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + data.len());
    buf.push(MSG_AWARENESS);
    buf.extend_from_slice(data);
    buf
}

/// Decode a raw WebSocket binary message into a SyncMessage.
/// Returns `Ok(None)` for non-sync messages (e.g., awareness — handled by #178).
pub fn decode_sync_message(data: &[u8]) -> Result<Option<SyncMessage>, SyncError> {
    if data.is_empty() {
        return Err(SyncError::EmptyMessage);
    }

    let msg_type = data[0];

    match msg_type {
        MSG_SYNC => {
            if data.len() < 2 {
                return Err(SyncError::TooShort);
            }
            let sub_type = data[1];
            let payload = data[2..].to_vec();

            match sub_type {
                MSG_SYNC_STEP1 => Ok(Some(SyncMessage::SyncStep1(payload))),
                MSG_SYNC_STEP2 => Ok(Some(SyncMessage::SyncStep2(payload))),
                MSG_SYNC_UPDATE => Ok(Some(SyncMessage::Update(payload))),
                other => Err(SyncError::UnknownSyncSubType(other)),
            }
        }
        MSG_AWARENESS => Ok(None),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_sync_step1() {
        let sv = vec![1, 2, 3, 4];
        let encoded = encode_sync_step1(&sv);
        let decoded = decode_sync_message(&encoded).unwrap().unwrap();
        assert_eq!(decoded, SyncMessage::SyncStep1(sv));
    }

    #[test]
    fn test_encode_decode_sync_step2() {
        let update = vec![10, 20, 30];
        let encoded = encode_sync_step2(&update);
        let decoded = decode_sync_message(&encoded).unwrap().unwrap();
        assert_eq!(decoded, SyncMessage::SyncStep2(update));
    }

    #[test]
    fn test_encode_decode_update() {
        let update = vec![5, 6, 7, 8, 9];
        let encoded = encode_update(&update);
        let decoded = decode_sync_message(&encoded).unwrap().unwrap();
        assert_eq!(decoded, SyncMessage::Update(update));
    }

    #[test]
    fn test_round_trip_via_sync_message_encode() {
        let msg = SyncMessage::SyncStep1(vec![42]);
        let encoded = msg.encode();
        let decoded = decode_sync_message(&encoded).unwrap().unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_reject_empty_input() {
        let result = decode_sync_message(&[]);
        assert!(matches!(result, Err(SyncError::EmptyMessage)));
    }

    #[test]
    fn test_reject_sync_too_short() {
        let result = decode_sync_message(&[MSG_SYNC]);
        assert!(matches!(result, Err(SyncError::TooShort)));
    }

    #[test]
    fn test_reject_unknown_sync_subtype() {
        let result = decode_sync_message(&[MSG_SYNC, 99]);
        assert!(matches!(result, Err(SyncError::UnknownSyncSubType(99))));
    }

    #[test]
    fn test_awareness_returns_none() {
        let data = vec![MSG_AWARENESS, 1, 2, 3];
        let result = decode_sync_message(&data).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_unknown_top_level_type_returns_none() {
        let data = vec![255, 1, 2];
        let result = decode_sync_message(&data).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_payload_sync_step1() {
        let encoded = encode_sync_step1(&[]);
        let decoded = decode_sync_message(&encoded).unwrap().unwrap();
        assert_eq!(decoded, SyncMessage::SyncStep1(vec![]));
    }
}
