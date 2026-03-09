use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, error, warn};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

use common::domain::DocumentRepository;

use crate::domain::active_document::ActiveDocument;
use crate::domain::content_extractor;

/// Shared in-memory cache of active documents being tracked by the snapshotter.
/// Uses std::sync::Mutex because yrs::Doc is !Send and all operations are synchronous.
pub type DocumentCache = Arc<Mutex<HashMap<String, ActiveDocument>>>;

/// Extracted document data for a compaction write.
/// (document_id, organization_id, yrs_state, yrs_state_vector, content_text, content_html)
type CompactionEntry = (String, String, Vec<u8>, Vec<u8>, String, String);

/// Stats returned by a compaction cycle.
#[derive(Debug, Default)]
pub struct CompactionStats {
    pub compacted: usize,
    pub skipped: usize,
    pub errors: usize,
}

pub struct SnapshotterService {
    cache: DocumentCache,
    document_repo: Arc<dyn DocumentRepository>,
}

impl SnapshotterService {
    pub fn new(document_repo: Arc<dyn DocumentRepository>, cache: DocumentCache) -> Self {
        Self {
            cache,
            document_repo,
        }
    }

    /// Apply a Yrs update for a document. Loads from PG on first access.
    pub async fn apply_update(&self, document_id: &str, update_bytes: &[u8]) -> anyhow::Result<()> {
        // Check if doc is already cached (brief lock)
        let in_cache = {
            let cache = self.cache.lock().unwrap();
            cache.contains_key(document_id)
        };

        if in_cache {
            // Apply to existing cached doc
            let mut cache = self.cache.lock().unwrap();
            if let Some(active) = cache.get_mut(document_id) {
                let update = Update::decode_v1(update_bytes)
                    .map_err(|e| anyhow::anyhow!("failed to decode yrs update: {}", e))?;
                let mut txn = active.doc.transact_mut();
                txn.apply_update(update)
                    .map_err(|e| anyhow::anyhow!("failed to apply update: {}", e))?;
                active.dirty = true;
                active.last_activity = Instant::now();
            }
            return Ok(());
        }

        // Not in cache — load from PostgreSQL (no lock held)
        let document = self
            .document_repo
            .get_document_by_id(document_id)
            .await
            .map_err(|e| anyhow::anyhow!("failed to load document from PG: {}", e))?;

        let document = match document {
            Some(doc) => doc,
            None => {
                warn!(document_id = %document_id, "document not found in PG, skipping update");
                return Ok(());
            }
        };

        // Build the Doc and apply updates (synchronous, no lock needed yet)
        let doc = Doc::new();
        if !document.yrs_state.is_empty() {
            let stored = Update::decode_v1(&document.yrs_state)
                .map_err(|e| anyhow::anyhow!("failed to decode stored yrs state: {}", e))?;
            let mut txn = doc.transact_mut();
            txn.apply_update(stored)
                .map_err(|e| anyhow::anyhow!("failed to apply stored state: {}", e))?;
        }
        {
            let update = Update::decode_v1(update_bytes)
                .map_err(|e| anyhow::anyhow!("failed to decode yrs update: {}", e))?;
            let mut txn = doc.transact_mut();
            txn.apply_update(update)
                .map_err(|e| anyhow::anyhow!("failed to apply new update: {}", e))?;
        }

        // Insert into cache (brief lock)
        let mut cache = self.cache.lock().unwrap();
        // Check again in case another task inserted while we were loading
        if !cache.contains_key(document_id) {
            cache.insert(
                document_id.to_string(),
                ActiveDocument {
                    doc,
                    organization_id: document.organization_id,
                    dirty: true,
                    last_activity: Instant::now(),
                },
            );
        } else if let Some(active) = cache.get_mut(document_id) {
            // Another task loaded it; apply our update to the cached version
            let update = Update::decode_v1(update_bytes)
                .map_err(|e| anyhow::anyhow!("failed to decode yrs update: {}", e))?;
            let mut txn = active.doc.transact_mut();
            txn.apply_update(update)
                .map_err(|e| anyhow::anyhow!("failed to apply update to cached doc: {}", e))?;
            active.dirty = true;
            active.last_activity = Instant::now();
        }

        Ok(())
    }

    /// Compact all dirty documents: encode state, extract content, write to PG.
    pub async fn compact_dirty_documents(&self) -> anyhow::Result<CompactionStats> {
        // Collect dirty document data under lock (synchronous Yrs operations).
        // Important: extract content BEFORE opening a transaction to avoid
        // yrs internal lock contention (get_or_insert_text needs write access).
        let dirty_docs: Vec<CompactionEntry> = {
            let cache = self.cache.lock().unwrap();
            cache
                .iter()
                .filter(|(_, active)| active.dirty)
                .map(|(id, active)| {
                    // Extract content first (may internally write to doc's type registry)
                    let content_text = content_extractor::extract_content_text(&active.doc);
                    let content_html = content_extractor::extract_content_html(&active.doc);
                    // Then encode state in a separate transaction
                    let txn = active.doc.transact();
                    let state = txn.encode_state_as_update_v1(&StateVector::default());
                    let state_vector = txn.state_vector().encode_v1();
                    (
                        id.clone(),
                        active.organization_id.clone(),
                        state,
                        state_vector,
                        content_text,
                        content_html,
                    )
                })
                .collect()
        };

        if dirty_docs.is_empty() {
            return Ok(CompactionStats::default());
        }

        debug!(dirty_count = dirty_docs.len(), "compacting dirty documents");

        let mut stats = CompactionStats::default();

        for (
            document_id,
            organization_id,
            yrs_state,
            yrs_state_vector,
            content_text,
            content_html,
        ) in &dirty_docs
        {
            let input = common::domain::UpdateYrsStateInput {
                document_id: document_id.clone(),
                organization_id: organization_id.clone(),
                yrs_state: yrs_state.clone(),
                yrs_state_vector: yrs_state_vector.clone(),
                content_text: content_text.clone(),
                content_html: content_html.clone(),
            };

            match self.document_repo.update_yrs_state(input).await {
                Ok(true) => {
                    // Mark clean in cache (brief lock)
                    {
                        let mut cache = self.cache.lock().unwrap();
                        if let Some(active) = cache.get_mut(document_id) {
                            active.dirty = false;
                        }
                    }
                    stats.compacted += 1;
                    debug!(document_id = %document_id, "compacted document");
                }
                Ok(false) => {
                    stats.skipped += 1;
                    debug!(document_id = %document_id, "advisory lock not acquired, skipped");
                }
                Err(e) => {
                    stats.errors += 1;
                    error!(document_id = %document_id, error = %e, "failed to compact document");
                }
            }
        }

        debug!(
            compacted = stats.compacted,
            skipped = stats.skipped,
            errors = stats.errors,
            "compaction cycle complete"
        );

        Ok(stats)
    }

    /// Evict idle documents from the cache.
    pub async fn evict_idle_documents(&self, idle_threshold: Duration) {
        let mut cache = self.cache.lock().unwrap();
        let before = cache.len();
        cache.retain(|id, active| {
            let idle = active.last_activity.elapsed() > idle_threshold;
            if idle && !active.dirty {
                debug!(document_id = %id, "evicting idle document from cache");
                false
            } else {
                true
            }
        });
        let evicted = before - cache.len();
        if evicted > 0 {
            debug!(
                evicted = evicted,
                remaining = cache.len(),
                "evicted idle documents"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::{Document, MockDocumentRepository};
    use common::yrs::create_empty_document;
    use yrs::Text;

    fn make_test_document(document_id: &str) -> Document {
        let (yrs_state, yrs_state_vector) = create_empty_document();
        Document {
            document_id: document_id.to_string(),
            organization_id: "org-1".to_string(),
            name: "Test".to_string(),
            yrs_state,
            yrs_state_vector,
            content_text: String::new(),
            content_html: String::new(),
            metadata: serde_json::json!({}),
            deleted_at: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        }
    }

    fn make_text_update(text: &str) -> Vec<u8> {
        let doc = Doc::new();
        let (state, _) = create_empty_document();
        {
            let mut txn = doc.transact_mut();
            let update = Update::decode_v1(&state).unwrap();
            txn.apply_update(update).unwrap();
        }
        {
            let t = doc.get_or_insert_text(common::yrs::ROOT_TEXT_NAME);
            let mut txn = doc.transact_mut();
            t.insert(&mut txn, 0, text);
            txn.encode_update_v1()
        }
    }

    #[tokio::test]
    async fn test_apply_update_loads_from_pg_on_first_access() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .returning(|id| Ok(Some(make_test_document(id))));

        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let service = SnapshotterService::new(Arc::new(mock_repo), cache.clone());

        let update = make_text_update("hello");
        service.apply_update("doc-1", &update).await.unwrap();

        let cache = cache.lock().unwrap();
        assert!(cache.contains_key("doc-1"));
        let active = cache.get("doc-1").unwrap();
        assert!(active.dirty);
        assert_eq!(active.organization_id, "org-1");
    }

    #[tokio::test]
    async fn test_apply_update_uses_cache_on_subsequent_calls() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .times(1) // Only loaded once from PG
            .returning(|id| Ok(Some(make_test_document(id))));

        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let service = SnapshotterService::new(Arc::new(mock_repo), cache.clone());

        let update1 = make_text_update("hello");
        service.apply_update("doc-1", &update1).await.unwrap();

        let update2 = make_text_update("world");
        service.apply_update("doc-1", &update2).await.unwrap();
    }

    #[tokio::test]
    async fn test_apply_update_skips_nonexistent_document() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .returning(|_| Ok(None));

        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let service = SnapshotterService::new(Arc::new(mock_repo), cache.clone());

        let update = make_text_update("hello");
        service.apply_update("doc-missing", &update).await.unwrap();

        let cache = cache.lock().unwrap();
        assert!(!cache.contains_key("doc-missing"));
    }

    #[tokio::test]
    async fn test_compact_dirty_documents() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .returning(|id| Ok(Some(make_test_document(id))));
        mock_repo.expect_update_yrs_state().returning(|_| Ok(true));

        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let service = SnapshotterService::new(Arc::new(mock_repo), cache.clone());

        let update = make_text_update("hello compaction");
        service.apply_update("doc-1", &update).await.unwrap();

        let stats = service.compact_dirty_documents().await.unwrap();
        assert_eq!(stats.compacted, 1);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.errors, 0);

        // Should be marked clean
        let cache = cache.lock().unwrap();
        assert!(!cache.get("doc-1").unwrap().dirty);
    }

    #[tokio::test]
    async fn test_compact_skips_when_lock_not_acquired() {
        let mut mock_repo = MockDocumentRepository::new();
        mock_repo
            .expect_get_document_by_id()
            .returning(|id| Ok(Some(make_test_document(id))));
        mock_repo.expect_update_yrs_state().returning(|_| Ok(false));

        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let service = SnapshotterService::new(Arc::new(mock_repo), cache.clone());

        let update = make_text_update("hello");
        service.apply_update("doc-1", &update).await.unwrap();

        let stats = service.compact_dirty_documents().await.unwrap();
        assert_eq!(stats.compacted, 0);
        assert_eq!(stats.skipped, 1);

        // Should still be dirty
        let cache = cache.lock().unwrap();
        assert!(cache.get("doc-1").unwrap().dirty);
    }

    #[tokio::test]
    async fn test_evict_idle_documents() {
        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let mock_repo = MockDocumentRepository::new();
        let service = SnapshotterService::new(Arc::new(mock_repo), cache.clone());

        {
            let doc = Doc::new();
            let mut cache = cache.lock().unwrap();
            cache.insert(
                "doc-old".to_string(),
                ActiveDocument {
                    doc,
                    organization_id: "org-1".to_string(),
                    dirty: false,
                    last_activity: Instant::now() - Duration::from_secs(600),
                },
            );
        }

        service.evict_idle_documents(Duration::from_secs(300)).await;

        let cache = cache.lock().unwrap();
        assert!(!cache.contains_key("doc-old"));
    }

    #[tokio::test]
    async fn test_evict_does_not_remove_dirty_documents() {
        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let mock_repo = MockDocumentRepository::new();
        let service = SnapshotterService::new(Arc::new(mock_repo), cache.clone());

        {
            let doc = Doc::new();
            let mut cache = cache.lock().unwrap();
            cache.insert(
                "doc-dirty".to_string(),
                ActiveDocument {
                    doc,
                    organization_id: "org-1".to_string(),
                    dirty: true,
                    last_activity: Instant::now() - Duration::from_secs(600),
                },
            );
        }

        service.evict_idle_documents(Duration::from_secs(300)).await;

        let cache = cache.lock().unwrap();
        assert!(cache.contains_key("doc-dirty"));
    }
}
