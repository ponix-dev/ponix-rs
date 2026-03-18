use std::time::{Duration, Instant};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

use crate::domain::content_extractor;

/// In-memory wrapper for an active Yrs document being tracked by the snapshotter.
pub struct ActiveDocument {
    doc: Doc,
    organization_id: String,
    dirty: bool,
    last_activity: Instant,
}

impl ActiveDocument {
    pub fn new(doc: Doc, organization_id: String) -> Self {
        Self {
            doc,
            organization_id,
            dirty: true,
            last_activity: Instant::now(),
        }
    }

    /// Decode and apply a Yrs v1 update to the document.
    pub fn apply_update(&mut self, update_bytes: &[u8]) -> anyhow::Result<()> {
        let update = Update::decode_v1(update_bytes)
            .map_err(|e| anyhow::anyhow!("failed to decode yrs update: {}", e))?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update)
            .map_err(|e| anyhow::anyhow!("failed to apply update: {}", e))?;
        self.dirty = true;
        self.last_activity = Instant::now();
        Ok(())
    }

    /// Encode the full document state and state vector as v1 bytes.
    pub fn encode_state(&self) -> (Vec<u8>, Vec<u8>) {
        let txn = self.doc.transact();
        let state = txn.encode_state_as_update_v1(&StateVector::default());
        let state_vector = txn.state_vector().encode_v1();
        (state, state_vector)
    }

    /// Extract plain-text and HTML content from the document.
    pub fn extract_content(&self) -> (String, String) {
        let text = content_extractor::extract_content_text(&self.doc);
        let html = content_extractor::extract_content_html(&self.doc);
        (text, html)
    }

    pub fn organization_id(&self) -> &str {
        &self.organization_id
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub fn is_idle(&self, threshold: Duration) -> bool {
        self.last_activity.elapsed() > threshold
    }
}
