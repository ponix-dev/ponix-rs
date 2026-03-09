use std::time::Instant;
use yrs::Doc;

/// In-memory wrapper for an active Yrs document being tracked by the snapshotter.
pub struct ActiveDocument {
    pub doc: Doc,
    pub organization_id: String,
    pub dirty: bool,
    pub last_activity: Instant,
}
