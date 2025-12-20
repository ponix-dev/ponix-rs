/// Casbin RBAC model with domains (organizations as domains)
///
/// Request format: (user_id, org_id, resource, action)
/// Policy format: (role, org_pattern, resource, action)
/// Role assignment: (user_id, role, org_id)
///
/// The matcher includes `g(r.sub, "super_admin", "*")` to allow super admins
/// access to all resources across all organizations.
///
/// We use `keyMatch(r.dom, p.dom)` instead of `r.dom == p.dom` to allow
/// policies with wildcard domain "*" to match any specific org_id.
pub const RBAC_MODEL: &str = r#"
[request_definition]
r = sub, dom, obj, act

[policy_definition]
p = sub, dom, obj, act

[role_definition]
g = _, _, _

[policy_effect]
e = some(where (p.eft == allow))

[matchers]
m = g(r.sub, p.sub, r.dom) && keyMatch(r.dom, p.dom) && r.obj == p.obj && r.act == p.act || g(r.sub, "super_admin", "*")
"#;
