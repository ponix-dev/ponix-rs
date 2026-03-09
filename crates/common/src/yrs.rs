use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact};

/// Name of the root text type in every Ponix Yrs document.
/// The collaboration server and snapshotter rely on this convention.
pub const ROOT_TEXT_NAME: &str = "content";

/// Creates a new empty Yrs document with a root text type and returns
/// the serialized state and state vector.
///
/// Every document created via the API uses this function to initialize
/// a well-formed Yrs document that downstream consumers (collaboration
/// server, snapshotter) can load and work with.
pub fn create_empty_document() -> (Vec<u8>, Vec<u8>) {
    let doc = Doc::new();
    let _text = doc.get_or_insert_text(ROOT_TEXT_NAME);

    let txn = doc.transact();
    let state = txn.encode_state_as_update_v1(&StateVector::default());
    let state_vector = txn.state_vector().encode_v1();

    (state, state_vector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::updates::decoder::Decode;
    use yrs::{GetString, Update};

    #[test]
    fn test_create_empty_document_returns_valid_state() {
        let (state, state_vector) = create_empty_document();
        assert!(!state.is_empty());
        assert!(!state_vector.is_empty());
    }

    #[test]
    fn test_create_empty_document_state_can_be_loaded() {
        let (state, _) = create_empty_document();
        let doc = Doc::new();
        let mut txn = doc.transact_mut();
        let update = Update::decode_v1(&state).expect("should decode");
        txn.apply_update(update).expect("should apply");
    }

    #[test]
    fn test_create_empty_document_has_root_text_type() {
        let (state, _) = create_empty_document();
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let update = Update::decode_v1(&state).expect("should decode");
            txn.apply_update(update).expect("should apply");
        }
        let text = doc.get_or_insert_text(ROOT_TEXT_NAME);
        let txn = doc.transact();
        assert_eq!(text.get_string(&txn), "");
    }

    #[test]
    fn test_state_vector_matches_document() {
        let (state, sv_bytes) = create_empty_document();
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let update = Update::decode_v1(&state).expect("should decode");
            txn.apply_update(update).expect("should apply");
        }
        let txn = doc.transact();
        let sv = txn.state_vector().encode_v1();
        assert_eq!(sv, sv_bytes);
    }
}
