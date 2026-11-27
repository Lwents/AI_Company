mod agent;
mod api;
mod auth;
mod chat;
mod config;
mod gitflow;
mod harness;
mod history;
mod import;
mod llm;
mod mcp;
mod skills;
mod ops;
mod portal;
mod resources;
mod screen;
mod store;
mod tools;
mod types;
mod workflow;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tower_http::services::{ServeDir, ServeFile};

fn project_root() -> PathBuf {
    if let Ok(r) = std::env::var("AI_COMPANY_ROOT") {
        return PathBuf::from(r);
    }
    // Default: the repository root (parent of backend/).
    let compiled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    if compiled.join("web").exists() {
        let canon = compiled.canonicalize().unwrap_or(compiled);
        // Drop the Windows verbatim prefix (\\?\) — cmd.exe cannot use it as a working directory.
        let s = canon.to_string_lossy();
        return PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string());
    }
    std::env::current_dir().unwrap_or_default()
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetConsoleOutputCP(code_page: u32) -> i32;
}

#[tokio::main]
async fn main() {
    // Print Vietnamese correctly in the Windows console.
    #[cfg(windows)]
    unsafe {
        SetConsoleOutputCP(65001);
    }
    // `ai-company hash-password`: reads a password from stdin, prints the AUTH_PASSWORD_HASH value.
    if std::env::args().nth(1).as_deref() == Some("hash-password") {
        let mut pw = String::new();
        let _ = std::io::stdin().read_line(&mut pw);
        let pw = pw.trim_end_matches(['\r', '\n']);
        if pw.chars().count() < 10 {
            eprintln!("Mật khẩu quá ngắn (tối thiểu 10 ký tự)");
            std::process::exit(1);
        }
        println!("{}", auth::hash_password(pw));
        return;
    }
    // Other processes of the same user (e.g. code the agents run) must not read this
    // process's memory or /proc/<pid>/environ, where API keys live.
    #[cfg(target_os = "linux")]
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }
    let root = project_root();
    let _ = dotenvy::from_path(root.join(".env"));
    let login = auth::init();
    let store = store::Store::load(&root);

    // Persist state periodically (only when something changed).
    {
        let store = store.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(800)).await;
                store.save_if_dirty();
            }
        });
    }

    // Customers, support, maintenance for launched products.
    ops::spawn(store.clone());

    // Automatic restarts after a failure that were still pending when the server stopped.
    workflow::resume_pending_retries(&store);
    // On a server, an update restarts the process: carry on with the projects it interrupted.
    if std::env::var("RESUME_ON_START").is_ok_and(|v| v == "1") {
        let ids = std::mem::take(&mut *store.interrupted.lock().unwrap());
        if !ids.is_empty() {
            let store = store.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                for id in ids {
                    if store.project(&id).is_some_and(|p| p.status == types::ProjectStatus::Paused) {
                        store.post_message(Some(&id), "system", "all", "Hệ thống vừa cập nhật phiên bản mới, dự án tự chạy tiếp từ bước đang dở.", "chat");
                        workflow::start(&store, &id);
                    }
                }
            });
        }
    }

    // Optional public address for customers (portal + product only), e.g. PORTAL_BIND=0.0.0.0:8790
    if let Ok(bind) = std::env::var("PORTAL_BIND") {
        match bind.parse::<SocketAddr>() {
            Ok(addr) => match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => {
                    println!("Cổng khách hàng công khai: http://{addr}/portal/<mã dự án>/");
                    let public = api::public_router(store.clone());
                    tokio::spawn(async move {
                        let _ = axum::serve(l, public).await;
                    });
                }
                Err(e) => eprintln!("Không mở được PORTAL_BIND {addr}: {e}"),
            },
            Err(_) => eprintln!("PORTAL_BIND không hợp lệ: {bind}"),
        }
    }

    let dist = root.join("web").join("dist");
    let app = api::router(store.clone())
        .fallback_service(ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html"))))
        // Login (when configured) guards every route, the static UI included.
        .layer(axum::middleware::from_fn(auth::guard));

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8787);
    // Localhost only: agents can run commands, so the API must never be exposed to the network.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Không mở được cổng {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("AI Company đang chạy: http://{addr}");
    println!("Thư mục dự án: {}", root.display());
    println!("{}", if login { "Đăng nhập: BẬT" } else { "Đăng nhập: tắt (chỉ dùng trên máy này)" });
    if !dist.exists() {
        println!("(Chưa build giao diện: chạy `npm run build` hoặc dùng `npm run dev` ở cổng 5173)");
    }

    let shutdown_store = store.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Ctrl+C, or SIGTERM from systemd on Linux.
            #[cfg(unix)]
            {
                let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            shutdown_store.save_if_dirty();
        })
        .await
        .unwrap();
    store.save_if_dirty();
}
