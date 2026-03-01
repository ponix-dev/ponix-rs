/// Resource types for authorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    DataStream,
    Gateway,
    Organization,
    Workspace,
}

impl Resource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Resource::DataStream => "data_stream",
            Resource::Gateway => "gateway",
            Resource::Organization => "organization",
            Resource::Workspace => "workspace",
        }
    }
}

/// Actions that can be performed on resources
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Read,
    Update,
    Delete,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Create => "create",
            Action::Read => "read",
            Action::Update => "update",
            Action::Delete => "delete",
        }
    }
}

/// Organization roles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgRole {
    Admin,
    Member,
}

impl OrgRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrgRole::Admin => "admin",
            OrgRole::Member => "member",
        }
    }
}

/// Base policies for roles (applied to wildcard domain, matched against specific org)
///
/// These policies define what each role can do. The domain "*" means these policies
/// apply to any organization where the user has the corresponding role.
pub fn base_policies() -> Vec<Vec<String>> {
    vec![
        // Admin policies - full CRUD on all resources
        vec![
            "admin".into(),
            "*".into(),
            "data_stream".into(),
            "create".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "data_stream".into(),
            "read".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "data_stream".into(),
            "update".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "data_stream".into(),
            "delete".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "gateway".into(),
            "create".into(),
        ],
        vec!["admin".into(), "*".into(), "gateway".into(), "read".into()],
        vec![
            "admin".into(),
            "*".into(),
            "gateway".into(),
            "update".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "gateway".into(),
            "delete".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "organization".into(),
            "read".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "organization".into(),
            "update".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "organization".into(),
            "delete".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "workspace".into(),
            "create".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "workspace".into(),
            "read".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "workspace".into(),
            "update".into(),
        ],
        vec![
            "admin".into(),
            "*".into(),
            "workspace".into(),
            "delete".into(),
        ],
        // Member policies (same as admin for now, designed for future differentiation)
        vec![
            "member".into(),
            "*".into(),
            "data_stream".into(),
            "create".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "data_stream".into(),
            "read".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "data_stream".into(),
            "update".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "data_stream".into(),
            "delete".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "gateway".into(),
            "create".into(),
        ],
        vec!["member".into(), "*".into(), "gateway".into(), "read".into()],
        vec![
            "member".into(),
            "*".into(),
            "gateway".into(),
            "update".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "gateway".into(),
            "delete".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "organization".into(),
            "read".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "workspace".into(),
            "create".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "workspace".into(),
            "read".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "workspace".into(),
            "update".into(),
        ],
        vec![
            "member".into(),
            "*".into(),
            "workspace".into(),
            "delete".into(),
        ],
    ]
}
