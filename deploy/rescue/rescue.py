#!/usr/bin/env python3
"""AI Company rescue service — a tiny separate service (own systemd unit, own port) that stays up while the
main app is deploying, restarting or broken.

It also HOSTS the system maintainer's Claude CLI session (a real terminal, xterm.js in the browser): the
session lives here, so a deploy or a crash of the main app never kills it, and the same terminal is reachable
from the main web (Khôi's window) and from /rescue/ when the main app is down.

Pages: status + log, restart, roll back to the previous build, and the terminal. Login: the main app's
session cookie (checked against data/sessions.json, which only holds hashes) or this page's own login
(same user / argon2 hash as the app).

Standard-library Python (plus python3-argon2), installed outside the CI deploy: a bad push can never take
the rescue page down with it. Env from /etc/ai-company/env. Listens on 127.0.0.1:${RESCUE_PORT:-18790};
nginx / Cloudflare forward /rescue/ to it.
"""
import base64
import glob
import signal
import fcntl
import hashlib
import hmac
import json
import os
import queue
import secrets
import shutil
import struct
import subprocess
import termios
import threading
import time
import urllib.request
from http import cookies
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import llm_router

try:
    from argon2 import PasswordHasher
except ImportError:  # pragma: no cover
    PasswordHasher = None

APP = os.environ.get("AI_COMPANY_ROOT", "/opt/ai-company")
DATA = os.path.join(APP, "data")
SRC = os.environ.get("AICO_SRC_DIR", os.path.join(APP, "src"))
MAIN = os.environ.get("RESCUE_MAIN_URL", "http://127.0.0.1:18787/login")
PORT = int(os.environ.get("RESCUE_PORT", "18790"))
STATIC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "static")
SECRET = secrets.token_bytes(32)  # this page's own sessions end when the rescue service restarts
SESSION_HOURS = 12
BACKLOG = 256 * 1024
# The CLI (about 300 MB) is only kept while used: closed after this many idle minutes, resumed (--continue) on demand.
IDLE_MIN = float(os.environ.get("RESCUE_CLI_IDLE_MIN", "15"))
WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


# ---------------------------------------------------------------- helpers
def sh(args, timeout=20):
    try:
        r = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
        return (r.stdout + r.stderr).strip(), r.returncode
    except Exception as e:  # noqa: BLE001
        return str(e), 1


def sign(user, exp):
    return hmac.new(SECRET, f"{user}|{exp}".encode(), hashlib.sha256).hexdigest()


def make_token(user):
    exp = int(time.time()) + SESSION_HOURS * 3600
    return f"{user}|{exp}|{sign(user, exp)}"


def valid_token(tok):
    try:
        user, exp, sig = tok.split("|")
        return int(exp) > time.time() and hmac.compare_digest(sig, sign(user, int(exp))) and user == os.environ.get("AUTH_USER", "")
    except Exception:  # noqa: BLE001
        return False


def main_session_ok(tok):
    """The main app's login cookie: data/sessions.json maps BLAKE2b-512(token)[:32] (hex) → expiry (ms)."""
    if not tok:
        return False
    try:
        with open(os.path.join(DATA, "sessions.json")) as f:
            sessions = json.load(f)
    except Exception:  # noqa: BLE001
        return False
    h = hashlib.blake2b(tok.encode()).digest()[:32].hex()
    exp = sessions.get(h)
    return bool(exp) and exp > time.time() * 1000


def check_password(user, pw):
    want_user = os.environ.get("AUTH_USER", "")
    hashed = os.environ.get("AUTH_PASSWORD_HASH", "").strip()
    if not want_user or not hashed or PasswordHasher is None:
        return False
    if not hmac.compare_digest(user.encode(), want_user.encode()):
        return False
    try:
        return PasswordHasher().verify(hashed, pw)
    except Exception:  # noqa: BLE001 — wrong password or bad hash (exception names differ across argon2 versions)
        return False


def status():
    active, _ = sh(["systemctl", "is-active", "ai-company"])
    since, _ = sh(["systemctl", "show", "ai-company", "-p", "ActiveEnterTimestamp", "--value"])
    try:
        code = urllib.request.urlopen(MAIN, timeout=4).status
        web, web_ok = f"trả lời (HTTP {code})", True
    except Exception as e:  # noqa: BLE001
        web, web_ok = f"KHÔNG trả lời ({str(e)[:80]})", False
    logs, _ = sh(["sudo", "-n", "/usr/bin/journalctl", "-u", "ai-company", "-n", "60", "--no-pager", "-o", "short-iso"])
    commits, _ = sh(["git", "-C", SRC, "log", "-6", "--format=%h %ad %an: %s", "--date=format:%d/%m %H:%M"])

    def mtime(p):
        try:
            return time.strftime("%d/%m %H:%M", time.localtime(os.path.getmtime(p)))
        except OSError:
            return "—"

    return {
        "active": active,
        "ok": active == "active" and web_ok,
        "since": since,
        "web": web,
        "logs": logs[-6000:],
        "commits": commits,
        "binary": mtime(os.path.join(APP, "ai-company")),
        "prev": mtime(os.path.join(APP, "ai-company.prev")),
        "cli": PTY.alive(),
        "cliIdleMin": IDLE_MIN,
    }


def restart():
    out, rc = sh(["sudo", "-n", "/usr/bin/systemctl", "restart", "ai-company"], timeout=60)
    return rc == 0, out or "Đã gửi lệnh khởi động lại."


def rollback():
    prev = os.path.join(APP, "ai-company.prev")
    if not os.path.isfile(prev):
        return False, "Không có bản trước (ai-company.prev)."
    cur = os.path.join(APP, "ai-company")
    shutil.copy2(cur, os.path.join(APP, "ai-company.bad"))
    shutil.copy2(prev, cur + ".new")
    os.replace(cur + ".new", cur)
    dist, dist_prev = os.path.join(APP, "web", "dist"), os.path.join(APP, "web", "dist.prev")
    if os.path.isdir(dist_prev):
        shutil.rmtree(dist + ".bad", ignore_errors=True)
        if os.path.isdir(dist):
            shutil.copytree(dist, dist + ".bad")
            shutil.rmtree(dist)
        shutil.copytree(dist_prev, dist)
    ok, out = restart()
    return ok, ("Đã quay về bản trước và khởi động lại. " if ok else "Đã chép bản trước nhưng khởi động lại lỗi: ") + out


# ---------------------------------------------------------------- the maintainer's Claude CLI (a real terminal)
def rules():
    return (
        "Bạn là nhân viên BẢO TRÌ HỆ THỐNG của AI Company, làm việc trên mã nguồn của chính hệ thống tại " + SRC + " (repo git, nhánh main). "
        "Chủ tịch nói chuyện với bạn qua terminal này (thường từ điện thoại); làm đúng yêu cầu, trả lời ngắn gọn bằng tiếng Việt, xưng em gọi anh/chị. "
        "Nếu hệ thống chính hỏng / không vào được: ưu tiên KHÔI PHỤC — xem log bằng `sudo -n /usr/bin/journalctl -u ai-company -n 200 --no-pager -o short-iso`, "
        "kiểm tra `systemctl is-active ai-company`, tìm nguyên nhân, sửa đúng chỗ; có thể `sudo -n /usr/bin/systemctl restart ai-company`. "
        "PHẠM VI: chỉ sửa trong thư mục mã nguồn; không đụng hệ điều hành, nginx, các dịch vụ khác (9router, xoay-opencode-api, lwent.cloud), không đọc / in bí mật. "
        "Máy KHÔNG có cargo: phần Rust do GitHub Actions build + test trước khi deploy (lỗi thì không deploy). Đầu việc `git pull --rebase origin main`; kiểm tra giao diện bằng `npm ci` (nếu thiếu node_modules) rồi `npm run typecheck && npm run build`. "
        "Sửa xong: commit (message tiếng Việt) rồi `git push origin main` — GitHub Actions deploy trong 2–3 phút, trừ khi Chủ tịch dặn chưa push. "
        "Terminal này chạy trong dịch vụ cứu hộ nên KHÔNG bị tắt khi hệ thống chính deploy / khởi động lại."
    )


def model_name(mid):
    base = mid.rsplit("/", 1)[-1].replace("(", " (")
    words = []
    for w in base.split("-"):
        if w == "free":
            continue
        words.append({"gemini": "Gemini", "deepseek": "DeepSeek"}.get(w, w.capitalize() if w in ("neon", "carbon", "argon", "hydrogen") else w))
    return " ".join(words)


def cli_env():
    key = os.environ.get("AICO_CLAUDE_KEY") or os.environ.get("CUSTOM_API_KEY", "")
    env = {
        "PATH": "/usr/local/bin:/usr/bin:/bin",
        "HOME": os.environ.get("AICO_CLAUDE_HOME", "/home/aico"),
        "LANG": "C.UTF-8",
        "TERM": "xterm-256color",
        "COLORTERM": "truecolor",
        "FORCE_COLOR": "3",
        "DISABLE_AUTOUPDATER": "1",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
        "GIT_AUTHOR_NAME": "Khôi (Bảo trì AI Company)",
        "GIT_COMMITTER_NAME": "Khôi (Bảo trì AI Company)",
        "GIT_AUTHOR_EMAIL": "maintainer@ai-company.local",
        "GIT_COMMITTER_EMAIL": "maintainer@ai-company.local",
    }
    # Through our own router: DeepSeek ("opencode/...") goes to Xoay, everything else to 9router.
    env["ANTHROPIC_BASE_URL"] = os.environ.get("AICO_CLAUDE_ROUTER_URL", f"http://127.0.0.1:{PORT}/llm")
    env["ANTHROPIC_AUTH_TOKEN"] = key
    main = os.environ.get("AICO_CLAUDE_MODEL", "").strip()
    mid = os.environ.get("AICO_CLAUDE_MID_MODEL", "").strip() or main
    small = os.environ.get("AICO_CLAUDE_SMALL_MODEL", "").strip() or mid
    deep = (main[: main.rfind("(")] if "(" in main else main) + "(high)" if main else ""
    for slot, model, note in [
        ("OPUS", main, "Mạnh nhất · model của CEO, BA, PM, Tech Lead trong dự án"),
        ("FABLE", deep, "Neon nghĩ sâu hơn · cho việc khó nhất (chậm hơn)"),
        ("SONNET", mid, "Cân bằng tốc độ / chất lượng"),
        ("HAIKU", small, "DeepSeek qua Xoay · nhanh, miễn phí · model của Dev, Tester…"),
    ]:
        if model:
            env[f"ANTHROPIC_DEFAULT_{slot}_MODEL"] = model
            env[f"ANTHROPIC_DEFAULT_{slot}_MODEL_NAME"] = model_name(model)
            env[f"ANTHROPIC_DEFAULT_{slot}_MODEL_DESCRIPTION"] = note
    fast = os.environ.get("AICO_CLAUDE_FAST_MODEL", "").strip() or small
    if fast:
        env["ANTHROPIC_SMALL_FAST_MODEL"] = fast  # background chores: a model that always answers quickly
    return env


class PtySession:
    """One Claude CLI for everybody; browsers attach and detach, the CLI keeps running."""

    def __init__(self):
        self.lock = threading.Lock()
        self.proc = None
        self.master = None
        self.backlog = bytearray()
        self.clients = set()
        self.last = time.time()
        self.idle_closed = False
        threading.Thread(target=self._reaper, daemon=True).start()

    def _reaper(self):
        """Closes the CLI after IDLE_MIN minutes with nobody attached and nothing happening."""
        while True:
            time.sleep(60)
            with self.lock:
                idle = self.alive() and not self.clients and time.time() - self.last > IDLE_MIN * 60
            if idle:
                self.idle_closed = True
                self.stop()

    def stop(self):
        p = self.proc
        if p is None or p.poll() is not None:
            return
        try:
            os.killpg(p.pid, signal.SIGTERM)
            p.wait(timeout=8)
        except Exception:  # noqa: BLE001
            try:
                os.killpg(p.pid, signal.SIGKILL)
            except OSError:
                pass

    @staticmethod
    def has_history():
        home = os.environ.get("AICO_CLAUDE_HOME", "/home/aico")
        return bool(glob.glob(os.path.join(home, ".claude", "projects", SRC.replace("/", "-"), "*.jsonl")))

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    def ensure(self, cols, rows):
        with self.lock:
            if self.alive():
                return
            master, slave = os.openpty()
            self._size(master, cols, rows)
            args = [os.environ.get("AICO_CLAUDE_BIN", "claude"), "--dangerously-skip-permissions", "--append-system-prompt", rules()]
            resumed = self.has_history()
            if resumed:
                args.append("--continue")  # pick the last conversation back up
            if os.environ.get("AICO_CLAUDE_MODEL"):
                args += ["--model", os.environ["AICO_CLAUDE_MODEL"]]

            def controlling_tty():
                os.setsid()
                fcntl.ioctl(0, termios.TIOCSCTTY, 0)

            self.proc = subprocess.Popen(args, cwd=SRC, env=cli_env(), stdin=slave, stdout=slave, stderr=slave, preexec_fn=controlling_tty, close_fds=True)
            os.close(slave)
            self.master = master
            self.backlog = bytearray()
            self.last = time.time()
            if self.idle_closed and resumed:
                self.backlog += "\x1b[2m[Phiên trước đã tự tắt khi không dùng để tiết kiệm RAM — mở lại và tiếp tục cuộc trò chuyện cũ…]\x1b[0m\r\n".encode()
            self.idle_closed = False
            threading.Thread(target=self._read, args=(master,), daemon=True).start()

    @staticmethod
    def _size(fd, cols, rows):
        try:
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", max(10, min(rows, 300)), max(40, min(cols, 400)), 0, 0))
        except OSError:
            pass

    def resize(self, cols, rows):
        if self.master is not None:
            self._size(self.master, cols, rows)

    def _read(self, fd):
        while True:
            try:
                data = os.read(fd, 8192)
            except OSError:
                break
            if not data:
                break
            with self.lock:
                self.last = time.time()
                # A full redraw (erase screen) makes everything before it obsolete: a browser that attaches
                # later replays only the current picture, not every earlier copy of the transcript.
                cut = data.rfind(b"[2J")
                if cut >= 0:
                    self.backlog = bytearray(data[cut:])
                else:
                    self.backlog += data
                if len(self.backlog) > BACKLOG:
                    del self.backlog[: len(self.backlog) - BACKLOG]
                for q in list(self.clients):
                    q.put(data)
        note = (
            "\r\n\x1b[2m[Claude CLI đã tự tắt vì không ai dùng (tiết kiệm RAM) — gõ phím bất kỳ để mở lại, cuộc trò chuyện được giữ nguyên]\x1b[0m\r\n"
            if self.idle_closed
            else "\r\n\x1b[2m[Claude CLI đã thoát — gõ phím bất kỳ để mở lại]\x1b[0m\r\n"
        ).encode()
        with self.lock:
            for q in list(self.clients):
                q.put(note)
        try:
            os.close(fd)
        except OSError:
            pass

    def write(self, text):
        self.last = time.time()
        if self.alive() and self.master is not None:
            try:
                os.write(self.master, text.encode())
            except OSError:
                pass

    def attach(self):
        q = queue.Queue()
        with self.lock:
            snapshot = bytes(self.backlog)
            self.clients.add(q)
        return q, snapshot

    def detach(self, q):
        with self.lock:
            self.clients.discard(q)
            self.last = time.time()


PTY = PtySession()


# ---------------------------------------------------------------- WebSocket (just what the terminal needs)
def ws_recv(rfile):
    head = rfile.read(2)
    if len(head) < 2:
        return None
    op, n = head[0] & 0x0F, head[1] & 0x7F
    if n == 126:
        n = struct.unpack(">H", rfile.read(2))[0]
    elif n == 127:
        n = struct.unpack(">Q", rfile.read(8))[0]
    if n > 1_000_000:
        return None
    mask = rfile.read(4) if head[1] & 0x80 else b"\0\0\0\0"
    data = bytearray(rfile.read(n))
    for i in range(len(data)):
        data[i] ^= mask[i % 4]
    return op, bytes(data)


def ws_frame(op, payload):
    n = len(payload)
    if n < 126:
        head = struct.pack(">BB", 0x80 | op, n)
    elif n < 65536:
        head = struct.pack(">BBH", 0x80 | op, 126, n)
    else:
        head = struct.pack(">BBQ", 0x80 | op, 127, n)
    return head + payload


# ---------------------------------------------------------------- pages
PAGE = r"""<!doctype html><html lang="vi"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover,interactive-widget=resizes-content">
<title>Cứu hộ AI Company</title><link rel="stylesheet" href="static/xterm.css">
<style>
:root{--bg:#0c0c0c;--bar:#161616;--line:#2b2b2b;--ink:#e6e6e6;--dim:#8a8a8a;--ok:#4ec98a;--bad:#ff6b6b;--acc:#e07a4f}
*{box-sizing:border-box}html,body{height:100%;margin:0;overflow:hidden;position:fixed;inset:0;width:100%;overscroll-behavior:none;background:var(--bg);color:var(--ink);font:14px/1.45 "Cascadia Mono",Consolas,ui-monospace,monospace}
body{display:flex;flex-direction:column;height:100dvh;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)}
header{display:flex;align-items:center;gap:6px;padding:6px 10px;background:var(--bar);border-bottom:1px solid var(--line);flex-wrap:wrap;flex:none}
header b{font-size:15px}.grow{flex:1}
.pill{padding:2px 8px;border-radius:99px;font-size:12px;background:#333}.pill.ok{background:#173d2a;color:var(--ok)}.pill.bad{background:#4a1d22;color:var(--bad)}
button,.btn{background:#222;color:var(--ink);border:1px solid var(--line);border-radius:6px;font:inherit;min-height:34px;padding:0 10px;cursor:pointer;text-decoration:none;display:inline-flex;align-items:center}
button:active{background:#333}.tabs{display:flex;gap:4px}.tabs button[aria-selected=true]{background:#2d3b55;border-color:#4a6496}
.imgbtn{background:#243248;border-color:#3c547a}
main{flex:1;min-height:0;display:flex;flex-direction:column}
#stage{position:relative;flex:1;min-height:0;display:flex;flex-direction:column}
#term{flex:1;min-height:0;overflow:hidden;padding:6px 4px 2px 8px}#term .xterm{height:100%}#term .xterm-viewport{overscroll-behavior:contain;-webkit-overflow-scrolling:touch;touch-action:pan-y}#term .xterm-helper-textarea{pointer-events:none}
#jump{position:absolute;right:12px;bottom:10px;z-index:5;background:#243248;border-color:#4a6496;border-radius:99px;box-shadow:0 4px 14px rgba(0,0,0,.5);font-size:12px;min-height:32px;padding:0 12px}
#jump[hidden],.keys[hidden]{display:none}
.keys{display:flex;gap:4px;padding:4px 8px;overflow-x:auto;border-top:1px solid var(--line);flex:none}
.keys button{min-height:34px;white-space:nowrap}
.line{display:flex;gap:6px;padding:4px 8px 8px;flex:none}.line input{flex:1;min-width:0;background:#141414;color:var(--ink);border:1px solid var(--line);border-radius:6px;font:inherit;font-size:16px;padding:6px 8px}
#st{overflow:auto;padding:12px;display:none;flex:1;min-height:0}#st.on{display:block}#st .row{display:flex;gap:8px;flex-wrap:wrap;margin:10px 0}
pre{background:#111;border:1px solid var(--line);border-radius:6px;padding:8px;white-space:pre-wrap;word-break:break-word;font-size:12px;max-height:50vh;overflow:auto}
.dim{color:var(--dim)}.ok{color:var(--ok)}.bad{color:var(--bad)}a{color:#7fa8ff}
@media (max-width:600px){header .hide-s{display:none}html,body{font-size:13px}}
</style></head><body>
<header><b>🛟 Cứu hộ</b><span id="pill" class="pill">…</span><span class="grow"></span>
<div class="tabs"><button id="t1" aria-selected="true">Claude CLI</button><button id="t2" aria-selected="false">Tình trạng</button></div>
<a class="btn hide-s" href="/">Trang chính</a></header>
<main><div id="tw" style="display:flex;flex-direction:column;flex:1;min-height:0"><div id="stage"><div id="term"></div><button id="jump" type="button" hidden>↓ Xuống cuối</button></div>
<input id="imgf" type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden>
<div class="keys" id="keys"></div>
<div class="line" id="line" hidden><button id="imgb" type="button" class="imgbtn">Ảnh</button><input id="li" placeholder="Gõ tin nhắn (có dấu) rồi Gửi" enterkeyhint="send"><button id="lb">Gửi</button></div></div>
<section id="st"><div id="sum" class="dim">Đang kiểm tra…</div>
<div class="row"><button id="re">🔄 Khởi động lại hệ thống chính</button><button id="rb">⏪ Quay về bản trước</button><a class="btn" href="/">← Trang chính</a></div>
<p class="dim">Bản lỗi bị đẩy lên? Bấm "Quay về bản trước" để chạy lại ngay, rồi nhắn Khôi ở tab Claude CLI xem log và sửa.</p>
<details open><summary>Log hệ thống chính</summary><pre id="logs"></pre></details><details><summary>Commit gần nhất</summary><pre id="commits"></pre></details></section></main>
<script src="static/xterm.js"></script><script src="static/addon-fit.js"></script>
<script>
const $=id=>document.getElementById(id);const touch=matchMedia("(any-pointer: coarse)").matches||navigator.maxTouchPoints>0;
const H={"Content-Type":"application/json","X-Rescue":"1"};const post=u=>fetch("api/"+u,{method:"POST",headers:H,body:"{}"}).then(r=>r.json());
function tab(n){$("t1").setAttribute("aria-selected",n===1);$("t2").setAttribute("aria-selected",n===2);$("tw").style.display=n===1?"flex":"none";$("st").classList.toggle("on",n===2);if(n===1){fit()}else refresh()}
$("t1").onclick=()=>tab(1);$("t2").onclick=()=>tab(2);
async function refresh(){try{const s=await fetch("api/status").then(r=>r.json());const p=$("pill");p.className="pill "+(s.ok?"ok":"bad");p.textContent=s.ok?"hệ thống chạy":"hệ thống KHÔNG chạy";
$("sum").innerHTML=`Dịch vụ: <b class="${s.active==="active"?"ok":"bad"}">${s.active}</b> từ ${s.since||"—"}<br>Web chính: <b class="${s.web.startsWith("trả lời")?"ok":"bad"}">${s.web}</b><br>Bản đang chạy: ${s.binary} · bản trước: ${s.prev}`;
const lg=$("logs");const atEnd=lg.scrollTop+lg.clientHeight>=lg.scrollHeight-20||!lg.dataset.seen;lg.textContent=s.logs||"(không đọc được log)";if(atEnd){lg.scrollTop=lg.scrollHeight;lg.dataset.seen=1}$("commits").textContent=s.commits}catch(e){$("pill").textContent="?"}}
$("re").onclick=async()=>{if(!confirm("Khởi động lại hệ thống chính?"))return;alert((await post("restart")).message);refresh()};
$("rb").onclick=async()=>{if(!confirm("Quay về bản trước? (bản hiện tại được giữ lại thành .bad)"))return;alert((await post("rollback")).message);refresh()};
const term=new Terminal({cursorBlink:true,fontFamily:'"Cascadia Mono",Consolas,"SF Mono",Menlo,ui-monospace,monospace',fontSize:innerWidth<600?12:14,lineHeight:1.15,scrollback:5000,allowProposedApi:true,theme:{background:"#0c0c0c",foreground:"#e6e6e6",cursor:"#e6e6e6"}});
const fa=new FitAddon.FitAddon();term.loadAddon(fa);term.open($("term"));
if(touch&&term.textarea){term.textarea.setAttribute("readonly","readonly");term.textarea.setAttribute("inputmode","none");term.textarea.tabIndex=-1}
term.parser.registerCsiHandler({final:"J"},p=>{if(p[0]===2)term.clear();return false});
let stick=true,typing=false,sent="",fitT=null;
const atBottom=()=>term.buffer.active.viewportY>=term.buffer.active.baseY-2;
function syncJump(){const a=atBottom();stick=a;$("jump").hidden=a||term.buffer.active.baseY<=0}
term.onScroll(syncJump);
function jumpBottom(){stick=true;$("jump").hidden=true;term.scrollToBottom()}
$("jump").onpointerdown=e=>e.preventDefault();$("jump").onclick=jumpBottom;
function fit(){try{fa.fit();if(stick){term.scrollToBottom();requestAnimationFrame(()=>{if(stick)term.scrollToBottom()})}}catch(e){}if(touch&&typing)return;const sz=term.cols+"x"+term.rows;if(sz!==sent&&ws&&ws.readyState===1){sent=sz;ws.send(JSON.stringify({type:"resize",cols:term.cols,rows:term.rows}))}}
let ws=null;function send(d){stick=true;$("jump").hidden=true;if(ws&&ws.readyState===1)ws.send(JSON.stringify({type:"input",data:d}))}
function connect(){ws=new WebSocket((location.protocol==="https:"?"wss":"ws")+"://"+location.host+"/rescue/pty?cols="+term.cols+"&rows="+term.rows);ws.binaryType="arraybuffer";
ws.onopen=()=>{stick=true;$("jump").hidden=true;term.reset();sent="";fit()};ws.onmessage=m=>{const done=()=>{if(stick)term.scrollToBottom();else syncJump()};term.write(typeof m.data==="string"?m.data:new Uint8Array(m.data),done)};ws.onclose=()=>{term.write("\r\n\x1b[2m[mất kết nối, đang nối lại…]\x1b[0m\r\n");setTimeout(connect,2000)}}
term.onData(send);new ResizeObserver(()=>{clearTimeout(fitT);fitT=setTimeout(fit,120)}).observe($("term"));fit();connect();
async function uploadImg(f){if(!f)return;const mb=$("imgb");if(mb){mb.disabled=true;mb.textContent="…"}try{const r=await fetch("api/upload",{method:"POST",headers:{"Content-Type":f.type||"application/octet-stream","X-Rescue":"1"},body:f});if(!r.ok){alert(await r.text()||"Không tải ảnh lên được");return}const d=await r.json();if(!d.path)return;if(touch){const i=$("li");i.value=i.value?i.value.trimEnd()+" "+d.path+" ":d.path+" ";i.focus()}else{send(d.path+" ");term.focus()}}catch(e){alert("Lỗi mạng khi gửi ảnh")}finally{if(mb){mb.disabled=false;mb.textContent="Ảnh"}}}
$("imgf").onchange=e=>{const f=e.target.files&&e.target.files[0];e.target.value="";if(f)uploadImg(f)};
window.addEventListener("paste",e=>{const it=e.clipboardData&&e.clipboardData.items;if(!it)return;for(const x of it){if(x.type.startsWith("image/")){const f=x.getAsFile();if(f){e.preventDefault();uploadImg(f);break}}}});
if(!touch){const ib=document.createElement("button");ib.textContent="Ảnh";ib.className="imgbtn";ib.onpointerdown=e=>e.preventDefault();ib.onclick=()=>$("imgf").click();$("keys").appendChild(ib)}
const jb=document.createElement("button");jb.textContent="↓ Cuối";jb.onpointerdown=e=>e.preventDefault();jb.onclick=jumpBottom;$("keys").appendChild(jb);
[["Esc","\x1b"],["Tab","\t"],["⇧Tab","\x1b[Z"],["↑","\x1b[A"],["↓","\x1b[B"],["←","\x1b[D"],["→","\x1b[C"],["Ctrl+C","\x03"],["⏎","\r"]].forEach(([l,s])=>{const b=document.createElement("button");b.textContent=l;b.onpointerdown=e=>e.preventDefault();b.onclick=()=>{send(s);if(!touch)term.focus()};$("keys").appendChild(b)});
if(touch){$("line").hidden=false;$("imgb").onpointerdown=e=>e.preventDefault();$("imgb").onclick=()=>$("imgf").click();const go=()=>{const v=$("li").value;if(!v.trim())return;send(v);setTimeout(()=>send("\r"),30);$("li").value=""};$("lb").onpointerdown=e=>e.preventDefault();$("lb").onclick=go;$("li").addEventListener("focus",()=>{typing=true;$("keys").hidden=true;jumpBottom();setTimeout(jumpBottom,180)});$("li").addEventListener("blur",()=>{typing=false;$("keys").hidden=false});$("li").addEventListener("keydown",e=>{if(e.key==="Enter"&&!e.isComposing){e.preventDefault();go()}});if(window.visualViewport){const sv=()=>{document.body.style.height=Math.round(visualViewport.height)+"px";document.body.style.transform="translate3d(0,"+Math.round(visualViewport.offsetTop)+"px,0)";if(stick)requestAnimationFrame(()=>term.scrollToBottom())};sv();visualViewport.addEventListener("resize",sv);visualViewport.addEventListener("scroll",sv)}}else term.focus();
if(location.hash==="#status")tab(2);refresh();setInterval(refresh,8000);
</script></body></html>"""

LOGIN = r"""<!doctype html><html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Cứu hộ AI Company</title>
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;background:#0c0c0c;color:#e6e6e6;font:15px "Cascadia Mono",Consolas,ui-monospace,monospace;padding:16px}
form{background:#161616;border:1px solid #2b2b2b;border-radius:10px;padding:20px;width:min(360px,100%);display:grid;gap:10px}
input,button{font:inherit;font-size:16px;padding:10px;border-radius:6px;border:1px solid #2b2b2b;background:#0c0c0c;color:#e6e6e6}button{background:#2d3b55;border-color:#4a6496}
.e{color:#ff6b6b}.d{color:#8a8a8a;font-size:13px}</style></head><body><form method="post" action="login"><b>🛟 Cứu hộ AI Company</b>
<span class="d">Đăng nhập bằng tài khoản AI Company (đã đăng nhập ở trang chính thì không cần).</span>ERR
<input name="u" placeholder="Tài khoản" autocomplete="username" required><input name="p" type="password" placeholder="Mật khẩu" autocomplete="current-password" required>
<button>Vào</button></form></body></html>"""


def allowed_origin(origin, host):
    if not origin:
        return True  # not a browser cross-site request
    allow = [o.strip().rstrip("/") for o in os.environ.get("ALLOWED_ORIGINS", "").split(",") if o.strip()]
    return origin in allow or origin in (f"https://{host}", f"http://{host}")


class Handler(BaseHTTPRequestHandler):
    server_version = "rescue"
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):  # quiet
        pass

    def path_only(self):
        p = self.path.split("?")[0]
        return p[len("/rescue"):] if p.startswith("/rescue") else p

    def query(self):
        from urllib.parse import parse_qs
        return {k: v[0] for k, v in parse_qs(self.path.split("?", 1)[1] if "?" in self.path else "").items()}

    def authed(self):
        c = cookies.SimpleCookie()
        try:
            c.load(self.headers.get("Cookie", ""))
        except cookies.CookieError:
            return False
        if "rescue" in c and valid_token(c["rescue"].value):
            return True
        return "aico_session" in c and main_session_ok(c["aico_session"].value)

    def send(self, code, body, ctype="application/json; charset=utf-8", headers=None):
        data = body.encode() if isinstance(body, str) else body
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store" if not ctype.startswith(("text/css", "application/javascript")) else "max-age=86400")
        self.send_header("X-Frame-Options", "DENY")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def j(self, obj, code=200):
        self.send(code, json.dumps(obj, ensure_ascii=False))

    def do_GET(self):
        p = self.path_only()
        if p.startswith("/llm/v1/"):
            return llm_router.handle(self, p[len("/llm"):], b"")
        if p == "":
            return self.send(302, "", headers={"Location": "/rescue/"})
        if p.startswith("/static/"):
            name = os.path.basename(p)
            path = os.path.join(STATIC, name)
            if name in ("xterm.js", "addon-fit.js", "xterm.css") and os.path.isfile(path):
                with open(path, "rb") as f:
                    return self.send(200, f.read(), "text/css" if name.endswith(".css") else "application/javascript")
            return self.j({"error": "Không có"}, 404)
        if p == "/":
            return self.send(200, PAGE if self.authed() else LOGIN.replace("ERR", ""), "text/html; charset=utf-8")
        if not self.authed():
            return self.j({"error": "Chưa đăng nhập"}, 401)
        if p == "/pty" and self.headers.get("Upgrade", "").lower() == "websocket":
            if not allowed_origin(self.headers.get("Origin"), self.headers.get("Host", "")):
                return self.j({"error": "Nguồn không hợp lệ"}, 403)
            return self.terminal()
        if p == "/api/status":
            return self.j(status())
        self.j({"error": "Không có"}, 404)

    def terminal(self):
        key = self.headers.get("Sec-WebSocket-Key", "")
        accept = base64.b64encode(hashlib.sha1((key + WS_GUID).encode()).digest()).decode()
        self.send_response(101, "Switching Protocols")
        self.send_header("Upgrade", "websocket")
        self.send_header("Connection", "Upgrade")
        self.send_header("Sec-WebSocket-Accept", accept)
        self.end_headers()
        self.wfile.flush()
        self.close_connection = True
        q = self.query()
        cols, rows = int(q.get("cols", 100) or 100), int(q.get("rows", 30) or 30)
        wlock = threading.Lock()
        alive = {"on": True}

        def out(op, payload):
            with wlock:
                try:
                    self.wfile.write(ws_frame(op, payload))
                    self.wfile.flush()
                except OSError:
                    alive["on"] = False

        try:
            PTY.ensure(cols, rows)
        except Exception as e:  # noqa: BLE001
            out(1, f"\r\nKhông mở được Claude CLI: {e}\r\n".encode())
            return
        PTY.resize(cols, rows)
        qq, snapshot = PTY.attach()
        if snapshot:
            out(2, snapshot)

        def pump():
            while alive["on"]:
                try:
                    data = qq.get(timeout=25)
                    out(2, data)
                except queue.Empty:
                    out(9, b"")  # ping: keeps proxies from closing an idle terminal

        t = threading.Thread(target=pump, daemon=True)
        t.start()
        try:
            while alive["on"]:
                fr = ws_recv(self.rfile)
                if fr is None:
                    break
                op, data = fr
                if op == 8:
                    break
                if op == 9:
                    out(10, data)
                    continue
                if op not in (1, 0):
                    continue
                try:
                    msg = json.loads(data.decode("utf-8", "replace"))
                except ValueError:
                    continue
                if msg.get("type") == "input":
                    if not PTY.alive():
                        PTY.ensure(cols, rows)
                        PTY.detach(qq)
                        qq, snap = PTY.attach()
                        continue
                    PTY.write(str(msg.get("data", "")))
                elif msg.get("type") == "resize":
                    cols, rows = int(msg.get("cols") or cols), int(msg.get("rows") or rows)
                    PTY.resize(cols, rows)
        except OSError:
            pass
        finally:
            alive["on"] = False
            PTY.detach(qq)

    def do_POST(self):
        p = self.path_only()
        if p.startswith("/llm/v1/"):
            n = min(int(self.headers.get("Content-Length") or 0), 64_000_000)
            return llm_router.handle(self, p[len("/llm"):], self.rfile.read(n))
        if p == "/api/upload":
            if not self.authed() or self.headers.get("X-Rescue") != "1":
                return self.j({"error": "Chưa đăng nhập"}, 401)
            n = int(self.headers.get("Content-Length") or 0)
            if n <= 0 or n > 20_000_000:
                return self.j({"error": "Ảnh quá lớn (tối đa 20 MB)"}, 400)
            raw = self.rfile.read(n)
            ext = (
                "png" if raw.startswith(b"\x89PNG\r\n\x1a\n")
                else "jpg" if raw.startswith(b"\xff\xd8\xff")
                else "gif" if raw.startswith((b"GIF87a", b"GIF89a"))
                else "webp" if len(raw) >= 12 and raw[:4] == b"RIFF" and raw[8:12] == b"WEBP"
                else ""
            )
            if not ext:
                return self.j({"error": "Chỉ nhận ảnh PNG, JPG, GIF hoặc WebP"}, 400)
            updir = os.path.join(DATA, "uploads", "chat")
            os.makedirs(updir, exist_ok=True)
            path = os.path.join(updir, f"img-{secrets.token_hex(6)}.{ext}")
            with open(path, "wb") as f:
                f.write(raw)
            return self.j({"path": path, "size": len(raw)})
        n = min(int(self.headers.get("Content-Length") or 0), 200_000)
        body = self.rfile.read(n).decode("utf-8", "replace")
        if p == "/login":
            from urllib.parse import parse_qs
            f = parse_qs(body)
            if check_password((f.get("u") or [""])[0], (f.get("p") or [""])[0]):
                return self.send(303, "", headers={"Location": "/rescue/", "Set-Cookie": f"rescue={make_token(os.environ.get('AUTH_USER',''))}; Path=/rescue; HttpOnly; Secure; SameSite=Strict; Max-Age={SESSION_HOURS*3600}"})
            time.sleep(1.5)
            return self.send(401, LOGIN.replace("ERR", '<span class="e">Sai tài khoản hoặc mật khẩu</span>'), "text/html; charset=utf-8")
        if not self.authed() or self.headers.get("X-Rescue") != "1":
            return self.j({"error": "Chưa đăng nhập"}, 401)
        if p == "/api/restart":
            ok, msg = restart()
            return self.j({"ok": ok, "message": msg})
        if p == "/api/rollback":
            try:
                ok, msg = rollback()
            except Exception as e:  # noqa: BLE001
                ok, msg = False, f"Lỗi: {e}"
            return self.j({"ok": ok, "message": msg})
        self.j({"error": "Không có"}, 404)


if __name__ == "__main__":
    os.makedirs(DATA, exist_ok=True)
    print(f"Rescue page on 127.0.0.1:{PORT}", flush=True)
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    srv.daemon_threads = True
    srv.serve_forever()
