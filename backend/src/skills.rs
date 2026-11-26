//! Skills: role playbooks (markdown) added to each employee's system prompt.
//! Files live in `skills/` at the company root and can be edited freely:
//! `common.md` (everyone in engineering) + `<role>.md` (e.g. `tester.md`, `data_engineer.md`).
use crate::mcp::role_key;
use crate::types::Role;
use std::path::Path;

/// Roles that also get the shared engineering playbook.
fn engineering(r: Role) -> bool {
    matches!(r, Role::Ceo | Role::Pm | Role::Ba | Role::Architect | Role::DataEngineer | Role::Security | Role::Designer | Role::Dev | Role::Reviewer | Role::Tester | Role::Devops)
}

pub fn for_role(root: &Path, role: Role) -> String {
    let dir = root.join("skills");
    let read = |name: &str| std::fs::read_to_string(dir.join(format!("{name}.md"))).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let mut parts = vec![];
    if engineering(role) {
        parts.extend(read("common"));
    }
    parts.extend(read(&role_key(role)));
    if parts.is_empty() {
        String::new()
    } else {
        format!("\n\n# Kỹ năng nghề của bạn\n{}", parts.join("\n\n"))
    }
}
