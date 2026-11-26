//! Skills: role playbooks (markdown) and per-employee custom skills added to each employee's system prompt.
//! Shared files live in `skills/` at the company root (`common.md` + `<role>.md`),
//! and each employee may additionally have inline `skill_text` and/or a linked `skill_file` (.md).
use crate::mcp::role_key;
use crate::types::{Employee, Role};
use std::path::{Path, PathBuf};

pub const MAX_SKILL_CHARS: usize = 40_000;

/// Roles that also get the shared engineering playbook.
fn engineering(r: Role) -> bool {
    matches!(r, Role::Ceo | Role::Pm | Role::Ba | Role::Architect | Role::DataEngineer | Role::Security | Role::Designer | Role::Dev | Role::Reviewer | Role::Tester | Role::Devops)
}

fn clamp_chars(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

fn allowed_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown" | "txt"))
        .unwrap_or(false)
}

/// Resolves and reads a `.md` / `.txt` skill file (absolute path, relative to company root, or inside `skills/`).
pub fn read_skill_file(root: &Path, raw: &str) -> Result<String, String> {
    let clean = raw.trim().trim_matches('"');
    if clean.is_empty() {
        return Ok(String::new());
    }
    let p = Path::new(clean);
    if !allowed_ext(p) {
        return Err("Tệp skill chỉ hỗ trợ định dạng .md, .markdown hoặc .txt".into());
    }
    let candidates: Vec<PathBuf> = if p.is_absolute() {
        vec![p.to_path_buf()]
    } else {
        vec![root.join(p), root.join("skills").join(p)]
    };
    let Some(found) = candidates.into_iter().find(|c| c.is_file()) else {
        return Err(format!("Không tìm thấy tệp skill: {clean}"));
    };
    let text = std::fs::read_to_string(&found).map_err(|e| format!("Không đọc được tệp skill {clean}: {e}"))?;
    Ok(clamp_chars(&text, MAX_SKILL_CHARS))
}

/// Lists built-in markdown skill files inside `root/skills/` as `(filename, relative_path, content)`.
pub fn list_files(root: &Path) -> Vec<(String, String, String)> {
    let dir = root.join("skills");
    let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
    let mut out = vec![];
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_file() && allowed_ext(&p) {
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = format!("skills/{name}");
            let content = std::fs::read_to_string(&p).map(|s| clamp_chars(&s, MAX_SKILL_CHARS)).unwrap_or_default();
            out.push((name, rel, content));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Builds the full skills section for one employee: shared role playbook + optional linked `.md` file + inline skill text.
pub fn for_employee(root: &Path, emp: &Employee) -> String {
    let dir = root.join("skills");
    let read = |name: &str| std::fs::read_to_string(dir.join(format!("{name}.md"))).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let mut parts = vec![];
    if engineering(emp.role) {
        parts.extend(read("common"));
    }
    parts.extend(read(&role_key(emp.role)));

    let file = emp.skill_file.trim().trim_matches('"');
    if !file.is_empty() {
        if let Ok(content) = read_skill_file(root, file) {
            if !content.is_empty() {
                parts.push(format!("## Kỹ năng bổ sung từ tệp ({file})\n{content}"));
            }
        }
    }
    let inline = clamp_chars(&emp.skill_text, MAX_SKILL_CHARS);
    if !inline.is_empty() {
        parts.push(format!("## Kỹ năng riêng của {}\n{inline}", emp.name));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("\n\n# Kỹ năng nghề của bạn\n{}", parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_role_file_and_inline_skills() {
        let tmp = std::env::temp_dir().join(format!("ai_company_skill_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("skills")).unwrap();
        std::fs::write(tmp.join("skills/dev.md"), "## Dev base").unwrap();
        std::fs::write(tmp.join("skills/custom.md"), "Luôn viết docstring ngắn gọn.").unwrap();

        assert!(read_skill_file(&tmp, "bad.exe").is_err());
        assert!(read_skill_file(&tmp, "missing.md").is_err());
        assert_eq!(read_skill_file(&tmp, "custom.md").unwrap(), "Luôn viết docstring ngắn gọn.");
        assert_eq!(read_skill_file(&tmp, "skills/custom.md").unwrap(), "Luôn viết docstring ngắn gọn.");

        let mut emp = crate::config::default_employees().into_iter().find(|e| e.role == Role::Dev).unwrap();
        emp.skill_file = "skills/custom.md".into();
        emp.skill_text = "Ưu tiên code Python thuần stdlib.".into();
        let combined = for_employee(&tmp, &emp);
        assert!(combined.contains("## Dev base"));
        assert!(combined.contains("Luôn viết docstring ngắn gọn."));
        assert!(combined.contains("Ưu tiên code Python thuần stdlib."));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
