//! The maintainer's real Claude CLI: an interactive session in a pseudo-terminal, streamed to the browser
//! (xterm.js) over a WebSocket. One session for the whole company; it keeps running when the browser
//! disconnects, and a new browser tab re-attaches to it (the recent screen output is replayed).
use crate::store::S;
use crate::types::*;
use axum::extract::ws::{Message, WebSocket};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::broadcast;

/// Screen output kept for re-attaching browsers.
const BACKLOG: usize = 256 * 1024;

struct Session {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    backlog: Mutex<VecDeque<u8>>,
    tx: broadcast::Sender<Vec<u8>>,
}

impl Session {
    fn alive(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }
}

fn slot() -> &'static Mutex<Option<Arc<Session>>> {
    static S: OnceLock<Mutex<Option<Arc<Session>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

/// The running session, or a new Claude CLI when there is none (or it has exited).
fn session(store: &S, cols: u16, rows: u16) -> Result<Arc<Session>, String> {
    let mut g = slot().lock().unwrap();
    if let Some(s) = g.as_ref().filter(|s| s.alive()) {
        return Ok(s.clone());
    }
    let dir = crate::maintainer::src_dir(store).ok_or("Chưa có thư mục mã nguồn AI Company (AICO_SRC_DIR)")?;
    let pair = native_pty_system().openpty(PtySize { rows: rows.max(10), cols: cols.max(40), pixel_width: 0, pixel_height: 0 }).map_err(|e| format!("Không mở được terminal: {e}"))?;
    let bin = std::env::var("AICO_CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
    let mut cmd = CommandBuilder::new(bin);
    cmd.arg("--dangerously-skip-permissions");
    cmd.arg("--append-system-prompt");
    cmd.arg(crate::maintainer::rules(&dir));
    if let Ok(m) = std::env::var("AICO_CLAUDE_MODEL") {
        cmd.arg("--model");
        cmd.arg(m);
    }
    cmd.cwd(&dir);
    let name = store.with(|s| s.employees.iter().find(|e| e.role == Role::Maintainer).map(|e| e.name.clone())).unwrap_or_else(|| "Bảo trì".into());
    if std::env::var("AICO_CLAUDE_HOME").is_ok() {
        // On the server: only what the CLI needs.
        cmd.env_clear();
    }
    for (k, v) in crate::maintainer::cli_env(&name) {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        }
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // A real colour terminal, whatever the server was started with (some shells export NO_COLOR=1).
    cmd.env_remove("NO_COLOR");
    cmd.env("FORCE_COLOR", "3");
    let child = pair.slave.spawn_command(cmd).map_err(|e| format!("Không chạy được Claude CLI: {e}"))?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let (tx, _) = broadcast::channel(512);
    let s = Arc::new(Session { master: Mutex::new(pair.master), writer: Mutex::new(writer), child: Mutex::new(child), backlog: Mutex::new(VecDeque::new()), tx });
    let s2 = s.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = buf[..n].to_vec();
                    {
                        let mut b = s2.backlog.lock().unwrap();
                        b.extend(chunk.iter().copied());
                        let over = b.len().saturating_sub(BACKLOG);
                        b.drain(..over);
                    }
                    let _ = s2.tx.send(chunk);
                }
            }
        }
        let note = b"\r\n\x1b[2m[Claude CLI \xc4\x91\xc3\xa3 tho\xc3\xa1t \xe2\x80\x94 g\xc3\xb5 Enter \xc4\x91\xe1\xbb\x83 m\xe1\xbb\x9f l\xe1\xba\xa1i]\x1b[0m\r\n".to_vec();
        let _ = s2.tx.send(note);
    });
    *g = Some(s.clone());
    Ok(s)
}

/// Serves one browser: replays the recent screen, then streams output and forwards keys / resizes.
pub async fn serve(store: S, mut socket: WebSocket, cols: u16, rows: u16) {
    let emp = store.with(|s| s.employees.iter().find(|e| e.role == Role::Maintainer && e.enabled).cloned());
    if emp.is_none() {
        let _ = socket.send(Message::Text("\r\nNhân viên Bảo trì đang tắt (bật ở tab Nhân sự).\r\n".into())).await;
        return;
    }
    let mut s = match session(&store, cols, rows) {
        Ok(s) => s,
        Err(e) => {
            let _ = socket.send(Message::Text(format!("\r\n{e}\r\n").into())).await;
            return;
        }
    };
    resize(&s, cols, rows);
    let snapshot: Vec<u8> = s.backlog.lock().unwrap().iter().copied().collect();
    if !snapshot.is_empty() && socket.send(Message::Binary(snapshot.into())).await.is_err() {
        return;
    }
    let mut rx = s.tx.subscribe();
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Ok(chunk) => if socket.send(Message::Binary(chunk.into())).await.is_err() { return },
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(t))) => {
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    match v.get("type").and_then(Value::as_str) {
                        Some("input") => {
                            // The CLI exited: any key starts a new one.
                            if !s.alive() {
                                match session(&store, cols, rows) {
                                    Ok(n) => { s = n; rx = s.tx.subscribe(); }
                                    Err(e) => { let _ = socket.send(Message::Text(format!("\r\n{e}\r\n").into())).await; }
                                }
                                continue;
                            }
                            let data = v.get("data").and_then(Value::as_str).unwrap_or("");
                            let mut w = s.writer.lock().unwrap();
                            let _ = w.write_all(data.as_bytes());
                            let _ = w.flush();
                        }
                        Some("resize") => {
                            let c = v.get("cols").and_then(Value::as_u64).unwrap_or(cols as u64) as u16;
                            let r = v.get("rows").and_then(Value::as_u64).unwrap_or(rows as u64) as u16;
                            resize(&s, c, r);
                        }
                        _ => {}
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            }
        }
    }
}

fn resize(s: &Session, cols: u16, rows: u16) {
    let _ = s.master.lock().unwrap().resize(PtySize { rows: rows.clamp(10, 300), cols: cols.clamp(40, 400), pixel_width: 0, pixel_height: 0 });
}
