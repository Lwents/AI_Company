mod agent;
mod api;
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
    let root = project_root();
    let _ = dotenvy::from_path(root.join(".env"));
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
    let app = api::router(store.clone()).fallback_service(ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html"))));

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
    if !dist.exists() {
        println!("(Chưa build giao diện: chạy `npm run build` hoặc dùng `npm run dev` ở cổng 5173)");
    }

    let shutdown_store = store.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown_store.save_if_dirty();
        })
        .await
        .unwrap();
    store.save_if_dirty();
}
