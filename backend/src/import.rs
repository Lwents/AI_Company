//! Bringing a customer's existing source code into a project: an uploaded .zip or a Git repository.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Folders never imported (dependencies and build output are recreated by the team).
const SKIP: [&str; 9] = ["node_modules", "dist", "build", "target", ".next", ".nuxt", ".cache", "coverage", "__MACOSX"];
const MAX_FILES: usize = 20_000;
const MAX_TOTAL: u64 = 500 * 1024 * 1024;

/// Extracts a zip into `dir`. A single top-level folder is flattened (zips of a folder are common).
pub fn extract_zip(bytes: &[u8], dir: &Path) -> Result<usize, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("Tệp .zip không hợp lệ: {e}"))?;
    // Common top-level folder?
    let mut top: Option<String> = None;
    let mut single = true;
    for i in 0..z.len() {
        let f = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(p) = f.enclosed_name() else { continue };
        let first = p.components().next().map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or_default();
        let is_root_file = p.components().count() == 1 && !f.is_dir();
        match (&top, is_root_file) {
            (_, true) => single = false,
            (None, _) => top = Some(first),
            (Some(t), _) if *t != first => single = false,
            _ => {}
        }
    }
    let strip = if single { top } else { None };
    let mut count = 0usize;
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name() rejects absolute paths and "..": no writing outside the project.
        let Some(mut rel) = f.enclosed_name() else { continue };
        if let Some(s) = &strip {
            rel = rel.strip_prefix(s).map(PathBuf::from).unwrap_or(rel);
        }
        if rel.as_os_str().is_empty() || rel.components().any(|c| SKIP.contains(&c.as_os_str().to_string_lossy().as_ref())) {
            continue;
        }
        let out = dir.join(&rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        count += 1;
        total += f.size();
        if count > MAX_FILES || total > MAX_TOTAL {
            return Err("Mã nguồn quá lớn (tối đa 20.000 tệp / 500 MB sau khi bỏ node_modules, dist…)".into());
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(&out, buf).map_err(|e| e.to_string())?;
    }
    if count == 0 {
        return Err("Tệp .zip không có mã nguồn".into());
    }
    Ok(count)
}

/// Checks a folder on this computer the team will work in DIRECTLY. Returns its absolute path.
pub fn check_local(src: &Path, forbidden: &[PathBuf]) -> Result<PathBuf, String> {
    let src = plain(src.canonicalize().map_err(|_| "Không tìm thấy thư mục này trên máy".to_string())?);
    if !src.is_dir() {
        return Err("Đường dẫn không phải thư mục".into());
    }
    if src.parent().is_none() {
        return Err("Không nhận cả ổ đĩa, hãy chọn thư mục dự án".into());
    }
    for f in forbidden {
        if let Ok(f) = f.canonicalize().map(plain) {
            if src.starts_with(&f) || f.starts_with(&src) {
                return Err("Không thể dùng thư mục của chính AI Company".into());
            }
        }
    }
    let has_files = std::fs::read_dir(&src).map(|mut rd| rd.next().is_some()).unwrap_or(false);
    if !has_files {
        return Err("Thư mục trống".into());
    }
    Ok(src)
}

/// Windows `canonicalize` returns `\\?\D:\…`; keep the normal form for display and child processes.
fn plain(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

/// Only plain remote URLs: no local paths, no option injection.
pub fn valid_git_url(url: &str) -> bool {
    let u = url.trim();
    !u.starts_with('-') && !u.contains(char::is_whitespace) && (u.starts_with("https://") || u.starts_with("git@") || u.starts_with("ssh://"))
}

pub async fn git_clone(url: &str, dir: &Path) -> Result<(), String> {
    if !valid_git_url(url) {
        return Err("Link Git phải dạng https://… hoặc git@…".into());
    }
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["clone", "--depth", "1", "--", url.trim()]).arg(dir).env("GIT_TERMINAL_PROMPT", "0").kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(300), cmd.output())
        .await
        .map_err(|_| "Clone quá 5 phút".to_string())?
        .map_err(|e| format!("Không chạy được git: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git clone lỗi: {}", err.trim().chars().take(400).collect::<String>()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            for (n, c) in files {
                z.start_file(*n, o).unwrap();
                z.write_all(c.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn flattens_top_folder_and_skips_junk() {
        let dir = std::env::temp_dir().join(format!("imp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let z = make_zip(&[("app/src/main.js", "x"), ("app/package.json", "{}"), ("app/node_modules/a/i.js", "y"), ("../evil.txt", "no")]);
        assert_eq!(extract_zip(&z, &dir).unwrap(), 2);
        assert!(dir.join("src/main.js").is_file());
        assert!(dir.join("package.json").is_file());
        assert!(!dir.join("node_modules").exists());
        assert!(!dir.parent().unwrap().join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checks_local_folder() {
        let base = std::env::temp_dir().join(format!("loc-{}", std::process::id()));
        let src = base.join("proj");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&src).unwrap();
        assert!(check_local(&src, &[]).is_err(), "empty folder");
        std::fs::write(src.join("main.js"), "x").unwrap();
        let p = check_local(&src, &[]).unwrap();
        assert!(!p.to_string_lossy().starts_with(r"\\?\"));
        assert!(check_local(&src, &[base.clone()]).is_err(), "inside a forbidden folder");
        assert!(check_local(&base.join("missing"), &[]).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn git_urls() {
        assert!(valid_git_url("https://github.com/a/b.git"));
        assert!(valid_git_url("git@github.com:a/b.git"));
        assert!(!valid_git_url("--upload-pack=x"));
        assert!(!valid_git_url("C:\\secret"));
        assert!(!valid_git_url("file:///etc"));
    }
}
