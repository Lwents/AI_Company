//! Project history: every hand-over / release is frozen as a zip of the source code,
//! and the current source can be downloaded as a zip at any time.
use crate::store::{now_ms, S};
use crate::types::ProjectVersion;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Folders never shipped in a source archive (dependencies, build output, caches, VCS).
const SKIP_DIRS: [&str; 11] = [".playwright-mcp", "node_modules", ".git", "dist", "build", "target", ".cache", "coverage", ".next", ".nuxt", ".vite"];

fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(base, &p, out);
            }
        } else if p.is_file() {
            out.push(p);
        }
    }
}

/// Zips `dir` (source only) under a top folder named `root`. Returns (bytes, file count).
pub fn zip_dir(dir: &Path, root: &str) -> Result<(Vec<u8>, usize), String> {
    let mut files = vec![];
    walk(dir, dir, &mut files);
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for f in &files {
            let rel = f.strip_prefix(dir).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
            let bytes = std::fs::read(f).map_err(|e| format!("{rel}: {e}"))?;
            z.start_file(format!("{root}/{rel}"), opts).map_err(|e| e.to_string())?;
            z.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok((buf.into_inner(), files.len()))
}

pub fn version_file(store: &S, slug: &str, n: u32) -> PathBuf {
    store.data_dir.join("history").join(slug).join(format!("v{n}.zip"))
}

/// Freezes the project's current source as the next version. Never fails the workflow.
pub async fn snapshot(store: &S, pid: &str, label: &str) {
    let Some(p) = store.project(pid) else { return };
    let n = p.versions.last().map(|v| v.n + 1).unwrap_or(1);
    let dir = store.dir_of(&p);
    let root = format!("{}-v{n}", p.slug);
    let out = version_file(store, &p.slug, n);
    let res = tokio::task::spawn_blocking(move || -> Result<(u64, usize), String> {
        let (bytes, files) = zip_dir(&dir, &root)?;
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&out, &bytes).map_err(|e| e.to_string())?;
        Ok((bytes.len() as u64, files))
    })
    .await;
    match res {
        Ok(Ok((size, files))) => {
            let v = ProjectVersion { n, label: label.to_string(), ts: now_ms(), size, files: files as u32 };
            store.update_project(pid, |p| p.versions.push(v.clone()));
            store.post_message(Some(pid), "system", "all", format!("Đã lưu phiên bản v{n}: {label} ({files} tệp)"), "chat");
        }
        Ok(Err(e)) => store.log_activity("system", Some(pid), "error", &format!("Không lưu được phiên bản: {e}")),
        Err(e) => store.log_activity("system", Some(pid), "error", &format!("Không lưu được phiên bản: {e}")),
    }
}
