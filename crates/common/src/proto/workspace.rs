use crate::domain::Workspace;
use ponix_proto_prost::workspace::v1::Workspace as ProtoWorkspace;
use ponix_proto_prost::workspace::v1::WorkspaceStatus;

use super::datetime_to_timestamp;

/// Convert domain Workspace to protobuf Workspace
pub fn to_proto_workspace(workspace: Workspace) -> ProtoWorkspace {
    let status = if workspace.deleted_at.is_some() {
        WorkspaceStatus::Deleted as i32
    } else {
        WorkspaceStatus::Active as i32
    };

    ProtoWorkspace {
        id: workspace.id,
        name: workspace.name,
        organization_id: workspace.organization_id,
        status,
        deleted_at: datetime_to_timestamp(workspace.deleted_at),
        created_at: datetime_to_timestamp(workspace.created_at),
        updated_at: datetime_to_timestamp(workspace.updated_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_workspace_to_proto_active() {
        let now = Utc::now();
        let workspace = Workspace {
            id: "ws-123".to_string(),
            name: "Test Workspace".to_string(),
            organization_id: "org-123".to_string(),
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_workspace(workspace);

        assert_eq!(proto.id, "ws-123");
        assert_eq!(proto.name, "Test Workspace");
        assert_eq!(proto.organization_id, "org-123");
        assert_eq!(proto.status, WorkspaceStatus::Active as i32);
        assert!(proto.deleted_at.is_none());
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
    }

    #[test]
    fn test_domain_workspace_to_proto_deleted() {
        let now = Utc::now();
        let workspace = Workspace {
            id: "ws-456".to_string(),
            name: "Deleted Workspace".to_string(),
            organization_id: "org-123".to_string(),
            deleted_at: Some(now),
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_workspace(workspace);

        assert_eq!(proto.id, "ws-456");
        assert_eq!(proto.status, WorkspaceStatus::Deleted as i32);
        assert!(proto.deleted_at.is_some());
    }
}
