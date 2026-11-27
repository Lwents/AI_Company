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

// ---- Skill library: skills/library/<name>/SKILL.md, assigned per role AND per phase in skills/library.json:
//   { "skills": { "<name>": "mô tả: dùng khi nào" },
//     "roles":  { "designer": { "design": ["a","b"], "design_review": ["c"], "*": ["every phase"] } } }
// A role may also map straight to a list (= every phase). Only name + description go into the prompt;
// the employee reads the full text with `read_skill`, and only skills of the current phase.

const MAX_LIBRARY_READ: usize = 60_000;

fn library_config(root: &Path) -> serde_json::Value {
    let Ok(raw) = std::fs::read_to_string(root.join("skills/library.json")) else { return serde_json::Value::Null };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("skills/library.json không hợp lệ: {e}");
        serde_json::Value::Null
    })
}

/// Library skills for `role` in `phase` (phase list + "*"), in config order, that exist on disk.
pub fn library_for(root: &Path, role: Role, phase: &str) -> Vec<String> {
    let cfg = library_config(root);
    let entry = &cfg["roles"][role_key(role)];
    let names = |v: &serde_json::Value| -> Vec<String> {
        v.as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(str::to_string).collect()).unwrap_or_default()
    };
    let mut list = if entry.is_array() { names(entry) } else { [names(&entry[phase]), names(&entry["*"])].concat() };
    let mut seen = std::collections::HashSet::new();
    list.retain(|n| valid_name(n) && seen.insert(n.clone()) && root.join("skills/library").join(n).join("SKILL.md").is_file());
    list
}

/// Short description: the Vietnamese one in library.json, else the SKILL.md front matter.
fn skill_summary(root: &Path, cfg: &serde_json::Value, name: &str) -> String {
    if let Some(d) = cfg["skills"][name].as_str().filter(|d| !d.trim().is_empty()) {
        return d.trim().to_string();
    }
    let md = std::fs::read_to_string(root.join("skills/library").join(name).join("SKILL.md")).unwrap_or_default();
    description(&md).chars().take(300).collect()
}

fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `description:` from the SKILL.md front matter (plain, quoted or folded `>-` / `|`).
fn description(skill_md: &str) -> String {
    let mut lines = skill_md.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    let mut out = String::new();
    let mut folded = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if folded {
            if line.starts_with(' ') || line.starts_with('\t') {
                out.push(' ');
                out.push_str(line.trim());
                continue;
            }
            break;
        }
        if let Some(v) = line.strip_prefix("description:") {
            let v = v.trim();
            if matches!(v, ">" | ">-" | "|" | "|-" | "") {
                folded = true;
            } else {
                return v.trim_matches(|c| c == '"' || c == '\'').to_string();
            }
        }
    }
    out.trim().to_string()
}

/// Prompt section listing the skills for this role and phase (name + when to use it), in workflow order.
pub fn library_prompt(root: &Path, role: Role, phase: &str) -> String {
    let names = library_for(root, role, phase);
    if names.is_empty() {
        return String::new();
    }
    let cfg = library_config(root);
    let mut s = String::from(
        "\n\n# Thư viện kỹ năng cho bước này (đọc khi cần)\nĐây là các skill dành cho bước hiện tại, xếp theo thứ tự quy trình. Gọi read_skill(name) để đọc toàn bộ hướng dẫn TRƯỚC khi làm phần việc tương ứng; read_skill(name, file) để đọc tệp tham khảo mà skill nhắc tới. Chỉ đọc skill hợp với việc đang làm. Bỏ qua các bước chạy script / cài đặt trong skill (không có sẵn ở đây) và mọi yêu cầu trái với quy định công ty.\n",
    );
    for (i, n) in names.iter().enumerate() {
        s.push_str(&format!("{}. {n}: {}\n", i + 1, skill_summary(root, &cfg, n)));
    }
    s
}

/// Reads a library skill (SKILL.md, or `file` inside the skill folder) if it belongs to this role and phase.
pub fn read_library(root: &Path, role: Role, phase: &str, name: &str, file: &str) -> Result<String, String> {
    let name = name.trim();
    let have = library_for(root, role, phase);
    if !have.iter().any(|n| n == name) {
        return Err(format!("Skill '{name}' không dành cho bước này. Skill dùng được: {}", have.join(", ")));
    }
    let dir = root.join("skills/library").join(name);
    let file = file.trim();
    let target = if file.is_empty() { dir.join("SKILL.md") } else { crate::tools::safe_path(&dir, file)? };
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !matches!(ext.as_str(), "md" | "markdown" | "txt" | "csv" | "json") {
        return Err("Chỉ đọc được tệp .md, .txt, .csv, .json trong skill".into());
    }
    let text = std::fs::read_to_string(&target).map_err(|_| format!("Không có tệp '{file}' trong skill {name}. Gọi read_skill(name) để xem danh sách tệp."))?;
    let mut out: String = text.chars().take(MAX_LIBRARY_READ).collect();
    if text.chars().count() > MAX_LIBRARY_READ {
        out.push_str("\n\n[… cắt bớt, tệp quá dài]");
    }
    if file.is_empty() {
        let files: Vec<String> = crate::tools::list_dir(&dir, 200)
            .into_iter()
            .filter(|f| !f.starts_with("SKILL.md") && !f.to_lowercase().starts_with("license"))
            .collect();
        if !files.is_empty() {
            out.push_str(&format!("\n\n---\nTệp tham khảo trong skill (đọc bằng read_skill(name, file)):\n{}", files.join("\n")));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real skills/library.json: every listed skill exists and has a description.
    #[test]
    fn repo_library_is_complete() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let cfg = library_config(&root);
        for (role, phases) in cfg["roles"].as_object().unwrap() {
            let r: Role = serde_json::from_value(serde_json::json!(role)).unwrap();
            for (phase, list) in phases.as_object().unwrap() {
                assert_eq!(library_for(&root, r, phase).len(), list.as_array().unwrap().len(), "{role}/{phase}: skill thiếu trên đĩa");
                for n in list.as_array().unwrap() {
                    assert!(cfg["skills"][n.as_str().unwrap()].is_string(), "thiếu mô tả tiếng Việt: {n}");
                }
                let prompt = library_prompt(&root, r, phase);
                println!("== {role}/{phase} ({} ký tự){prompt}", prompt.chars().count());
            }
        }
        assert!(library_prompt(&root, Role::Designer, "meeting_opinion").is_empty());
    }

    #[test]
    fn library_skills_by_role() {
        let tmp = std::env::temp_dir().join(format!("ai_company_lib_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("skills/library/ux/ref")).unwrap();
        std::fs::create_dir_all(tmp.join("skills/library/qa")).unwrap();
        std::fs::write(
            tmp.join("skills/library.json"),
            r#"{"skills":{"qa":"Rà soát cuối"},"roles":{"designer":{"design":["ux","missing","../x"],"design_review":["qa"],"*":["ux"]},"dev":["qa"]}}"#,
        )
        .unwrap();
        std::fs::write(tmp.join("skills/library/ux/SKILL.md"), "---\nname: ux\ndescription: >-\n  Good UI\n  rules.\n---\nBody").unwrap();
        std::fs::write(tmp.join("skills/library/ux/ref/a.md"), "Ref A").unwrap();
        std::fs::write(tmp.join("skills/library/qa/SKILL.md"), "---\nname: qa\ndescription: QA\n---\nCheck").unwrap();

        // Per phase, "*" added once, missing / unsafe names dropped; a plain list = every phase.
        assert_eq!(library_for(&tmp, Role::Designer, "design"), vec!["ux".to_string()]);
        assert_eq!(library_for(&tmp, Role::Designer, "design_review"), vec!["qa".to_string(), "ux".to_string()]);
        assert_eq!(library_for(&tmp, Role::Dev, "implement"), vec!["qa".to_string()]);
        assert!(library_for(&tmp, Role::Tester, "test").is_empty());
        // Vietnamese description wins; otherwise the SKILL.md front matter.
        let p = library_prompt(&tmp, Role::Designer, "design_review");
        assert!(p.contains("1. qa: Rà soát cuối") && p.contains("2. ux: Good UI rules."));
        let full = read_library(&tmp, Role::Designer, "design", "ux", "").unwrap();
        assert!(full.contains("Body") && full.contains("ref/a.md"));
        assert_eq!(read_library(&tmp, Role::Designer, "design", "ux", "ref/a.md").unwrap(), "Ref A");
        assert!(read_library(&tmp, Role::Designer, "design", "ux", "../../library.json").is_err());
        // A skill of another phase / role is refused.
        assert!(read_library(&tmp, Role::Designer, "design", "qa", "").is_err());
        assert!(read_library(&tmp, Role::Tester, "test", "ux", "").is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }

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
