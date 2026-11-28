#!/usr/bin/env python3
"""AI Company rescue page — a tiny separate service (own systemd unit, own port) that stays up while the
main app is deploying, restarting or broken. The chairman can see what is wrong, restart, roll back to the
previous version, and ask the maintainer (Claude CLI) to fix the code.

Deliberately standard-library Python (plus python3-argon2) and installed outside the CI deploy: a bad push
can never take the rescue page down with it.

Env (from /etc/ai-company/env): AUTH_USER, AUTH_PASSWORD_HASH (argon2), AICO_* maintainer settings,
CUSTOM_API_KEY. Listens on 127.0.0.1:${RESCUE_PORT:-18790}; nginx / Cloudflare forward /rescue/ to it.
"""
import hashlib
import hmac
import html
import json
import os
import secrets
import shutil
import subprocess
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http import cookies

try:
    from argon2 import PasswordHasher
    from argon2.exceptions import VerifyMismatchError, InvalidHashError
except ImportError:  # pragma: no cover
    PasswordHasher = None

APP = os.environ.get("AI_COMPANY_ROOT", "/opt/ai-company")
DATA = os.path.join(APP, "data")
SRC = os.environ.get("AICO_SRC_DIR", os.path.join(APP, "src"))
MAIN = os.environ.get("RESCUE_MAIN_URL", "http://127.0.0.1:18787/login")
PORT = int(os.environ.get("RESCUE_PORT", "18790"))
SECRET = secrets.token_bytes(32)  # sessions end when the rescue service restarts
SESSION_HOURS = 12
CHAT_FILE = os.path.join(DATA, "rescue-chat.json")
BUSY_FILE = os.path.join(DATA, "maintainer.busy")
SESSION_FILE = os.path.join(DATA, "maintainer.json")
LOCK = threading.Lock()
RUN = {"proc": None}


# ---------------------------------------------------------------- helpers
def sh(args, timeout=20):
    try:
        r = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
        return (r.stdout + r.stderr).strip(), r.returncode
    except Exception as e:  # noqa: BLE001
        return str(e), 1


def sign(user, exp):
    msg = f"{user}|{exp}".encode()
    return hmac.new(SECRET, msg, hashlib.sha256).hexdigest()


def make_token(user):
    exp = int(time.time()) + SESSION_HOURS * 3600
    return f"{user}|{exp}|{sign(user, exp)}"


def valid_token(tok):
    try:
        user, exp, sig = tok.split("|")
        return int(exp) > time.time() and hmac.compare_digest(sig, sign(user, int(exp))) and user == os.environ.get("AUTH_USER", "")
    except Exception:  # noqa: BLE001
        return False


def check_password(user, pw):
    want_user = os.environ.get("AUTH_USER", "")
    hashed = os.environ.get("AUTH_PASSWORD_HASH", "").strip()
    if not want_user or not hashed or PasswordHasher is None:
        return False
    if not hmac.compare_digest(user.encode(), want_user.encode()):
        return False
    try:
        return PasswordHasher().verify(hashed, pw)
    except (VerifyMismatchError, InvalidHashError, Exception):  # noqa: BLE001
        return False


def load_chat():
    try:
        with open(CHAT_FILE, encoding="utf-8") as f:
            return json.load(f)
    except Exception:  # noqa: BLE001
        return []


def add_chat(who, text):
    with LOCK:
        log = load_chat()
        log.append({"who": who, "text": text, "ts": int(time.time())})
        log = log[-80:]
        with open(CHAT_FILE, "w", encoding="utf-8") as f:
            json.dump(log, f, ensure_ascii=False)


def pid_alive(pid):
    return pid > 0 and os.path.exists(f"/proc/{pid}")


def busy_pid():
    try:
        pid = int(open(BUSY_FILE).read().strip() or 0)
    except Exception:  # noqa: BLE001
        return 0
    return pid if pid_alive(pid) else 0


def status():
    active, _ = sh(["systemctl", "is-active", "ai-company"])
    since, _ = sh(["systemctl", "show", "ai-company", "-p", "ActiveEnterTimestamp", "--value"])
    try:
        code = urllib.request.urlopen(MAIN, timeout=4).status
        web = f"trả lời (HTTP {code})"
    except Exception as e:  # noqa: BLE001
        web = f"KHÔNG trả lời ({str(e)[:80]})"
    logs, _ = sh(["sudo", "-n", "/usr/bin/journalctl", "-u", "ai-company", "-n", "60", "--no-pager", "-o", "short-iso"])
    commits, _ = sh(["git", "-C", SRC, "log", "-6", "--format=%h %ad %an: %s", "--date=format:%d/%m %H:%M"])

    def mtime(p):
        try:
            return time.strftime("%d/%m %H:%M", time.localtime(os.path.getmtime(p)))
        except OSError:
            return "—"

    return {
        "active": active,
        "since": since,
        "web": web,
        "logs": logs[-6000:],
        "commits": commits,
        "binary": mtime(os.path.join(APP, "ai-company")),
        "prev": mtime(os.path.join(APP, "ai-company.prev")),
        "busy": bool(busy_pid()),
    }


def restart():
    out, rc = sh(["sudo", "-n", "/usr/bin/systemctl", "restart", "ai-company"], timeout=60)
    return rc == 0, out or "Đã gửi lệnh khởi động lại."


def rollback():
    prev = os.path.join(APP, "ai-company.prev")
    if not os.path.isfile(prev):
        return False, "Không có bản trước (ai-company.prev)."
    cur = os.path.join(APP, "ai-company")
    bad = os.path.join(APP, "ai-company.bad")
    shutil.copy2(cur, bad)
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


# ---------------------------------------------------------------- maintainer (Claude CLI)
def rules():
    return (
        "Bạn là nhân viên BẢO TRÌ HỆ THỐNG của AI Company, làm việc trên mã nguồn tại " + SRC + " (repo git, nhánh main). "
        "Bạn đang được gọi từ TRANG CỨU HỘ: hệ thống chính có thể đang hỏng, đang deploy hoặc khởi động lại. Ưu tiên KHÔI PHỤC cho chạy lại: đọc log dịch vụ (đính kèm trong tin nhắn), tìm nguyên nhân, sửa đúng chỗ. "
        "Trả lời ngắn gọn bằng tiếng Việt, xưng em gọi anh/chị. "
        "PHẠM VI: chỉ sửa trong thư mục mã nguồn; không đụng hệ điều hành, nginx, các dịch vụ khác (9router, xoay-opencode-api, lwent.cloud), không đọc / in bí mật. "
        "Được phép: `sudo -n /usr/bin/systemctl restart ai-company` và `sudo -n /usr/bin/journalctl -u ai-company -n 200 --no-pager -o short-iso`. "
        "Máy KHÔNG có cargo: phần Rust do GitHub Actions build + test (lỗi thì không deploy). Đầu việc `git pull --rebase origin main`; kiểm tra giao diện bằng `npm ci` (nếu thiếu) rồi `npm run typecheck && npm run build`. "
        "Sửa xong: commit (message tiếng Việt) rồi `git push origin main` (GitHub Actions deploy trong 2–3 phút), trừ khi Chủ tịch dặn chưa push. Push xong báo lại và kết thúc lượt."
    )


def claude_env():
    key = os.environ.get("AICO_CLAUDE_KEY") or os.environ.get("CUSTOM_API_KEY", "")
    env = {
        "PATH": "/usr/local/bin:/usr/bin:/bin",
        "HOME": os.environ.get("AICO_CLAUDE_HOME", "/home/aico"),
        "LANG": "C.UTF-8",
        "TERM": "dumb",
        "DISABLE_AUTOUPDATER": "1",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
        "GIT_AUTHOR_NAME": "Khôi (Bảo trì AI Company)",
        "GIT_COMMITTER_NAME": "Khôi (Bảo trì AI Company)",
        "GIT_AUTHOR_EMAIL": "maintainer@ai-company.local",
        "GIT_COMMITTER_EMAIL": "maintainer@ai-company.local",
    }
    if os.environ.get("AICO_CLAUDE_BASE_URL"):
        env["ANTHROPIC_BASE_URL"] = os.environ["AICO_CLAUDE_BASE_URL"]
        env["ANTHROPIC_API_KEY"] = key
    small = os.environ.get("AICO_CLAUDE_SMALL_MODEL")
    if small:
        env["ANTHROPIC_DEFAULT_HAIKU_MODEL"] = small
        env["ANTHROPIC_SMALL_FAST_MODEL"] = small
    return env


def describe(name, inp):
    g = lambda k: str((inp or {}).get(k, ""))  # noqa: E731
    return {
        "Bash": lambda: "$ " + g("command")[:160],
        "Read": lambda: "Đọc " + g("file_path"),
        "Edit": lambda: "Sửa " + g("file_path"),
        "MultiEdit": lambda: "Sửa " + g("file_path"),
        "Write": lambda: "Ghi " + g("file_path"),
        "Grep": lambda: "Tìm \"" + g("pattern")[:60] + "\"",
        "Glob": lambda: "Tìm tệp " + g("pattern"),
    }.get(name, lambda: name)()


def run_claude(text):
    try:
        sess = json.load(open(SESSION_FILE)).get("session") or None
    except Exception:  # noqa: BLE001
        sess = None
    logs, _ = sh(["sudo", "-n", "/usr/bin/journalctl", "-u", "ai-company", "-n", "40", "--no-pager", "-o", "short-iso"])
    st = status()
    prompt = f"{text}\n\n[Tình trạng hệ thống lúc này: dịch vụ {st['active']}, web {st['web']}]\n[40 dòng log gần nhất của ai-company]\n{logs[-4000:]}"
    args = [os.environ.get("AICO_CLAUDE_BIN", "claude"), "-p", prompt, "--output-format", "stream-json", "--verbose", "--dangerously-skip-permissions", "--append-system-prompt", rules()]
    if sess:
        args += ["--resume", sess]
    if os.environ.get("AICO_CLAUDE_MODEL"):
        args += ["--model", os.environ["AICO_CLAUDE_MODEL"]]
    started = time.time()
    try:
        p = subprocess.Popen(args, cwd=SRC, env=claude_env(), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except Exception as e:  # noqa: BLE001
        add_chat("khoi", f"⚠️ Không chạy được Claude CLI: {e}")
        return
    RUN["proc"] = p
    with open(BUSY_FILE, "w") as f:
        f.write(str(p.pid))
    last, result = "", None
    try:
        for line in p.stdout:
            try:
                ev = json.loads(line)
            except ValueError:
                continue
            sid = ev.get("session_id")
            if sid and sid != sess:
                sess = sid
                json.dump({"session": sid}, open(SESSION_FILE, "w"))
            if ev.get("type") == "assistant":
                for c in (ev.get("message") or {}).get("content") or []:
                    if c.get("type") == "text" and c.get("text", "").strip():
                        last = c["text"].strip()
                        add_chat("khoi", last)
                    elif c.get("type") == "tool_use":
                        add_chat("tool", describe(c.get("name"), c.get("input")))
            elif ev.get("type") == "result":
                result = ev
            if time.time() - started > 3600:
                p.kill()
                add_chat("khoi", "⏹ Quá 60 phút, em dừng lại.")
                break
        p.wait(timeout=30)
    except Exception as e:  # noqa: BLE001
        add_chat("khoi", f"⚠️ Lỗi khi chạy: {e}")
    finally:
        RUN["proc"] = None
        try:
            os.remove(BUSY_FILE)
        except OSError:
            pass
    mins = (time.time() - started) / 60
    if result is None:
        if p.returncode not in (0, None) and p.returncode != -9:
            add_chat("khoi", f"⚠️ Claude CLI dừng giữa chừng (mã {p.returncode}): {(p.stderr.read() or '')[-600:]}")
        return
    final = (result.get("result") or "").strip()
    if result.get("is_error"):
        add_chat("khoi", f"⚠️ Claude CLI báo lỗi: {final[:1500]}  ⏱ {mins:.1f} phút")
    elif final and final != last:
        add_chat("khoi", f"{final}\n\n⏱ {mins:.1f} phút")
    else:
        add_chat("khoi", f"✅ Xong. ⏱ {mins:.1f} phút")


# ---------------------------------------------------------------- web
PAGE = r"""<!doctype html><html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Cứu hộ AI Company</title><style>
:root{--bg:#10131c;--card:#1a1f2d;--line:#2c3448;--ink:#e8ebf2;--muted:#8f99b0;--ok:#3ccf91;--bad:#ff6b6b;--warn:#ffb84d;--acc:#5b8cff}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}
main{max-width:860px;margin:0 auto;padding:16px}h1{font-size:20px;margin:4px 0 12px}h2{font-size:15px;margin:0 0 8px;color:var(--muted);text-transform:uppercase;letter-spacing:.04em}
.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:14px;margin-bottom:12px}
.row{display:flex;gap:8px;flex-wrap:wrap;align-items:center}.grow{flex:1}
button{background:var(--line);color:var(--ink);border:0;border-radius:8px;padding:10px 14px;font:inherit;cursor:pointer;min-height:42px}
button.p{background:var(--acc)}button.d{background:#7a2e35}button:disabled{opacity:.5}
pre{background:#0b0e15;border-radius:8px;padding:10px;overflow:auto;max-height:300px;font:12px/1.45 ui-monospace,Consolas,monospace;white-space:pre-wrap;word-break:break-word;margin:6px 0 0}
.ok{color:var(--ok)}.bad{color:var(--bad)}.warn{color:var(--warn)}.muted{color:var(--muted)}
textarea,input{width:100%;background:#0b0e15;color:var(--ink);border:1px solid var(--line);border-radius:8px;padding:10px;font:inherit}
.log{max-height:55vh;overflow:auto;display:flex;flex-direction:column;gap:6px}
.m{padding:8px 10px;border-radius:10px;white-space:pre-wrap;word-break:break-word}.m.me{background:#24345e;align-self:flex-end;max-width:90%}
.m.khoi{background:#202838;max-width:95%}.m.tool{color:var(--muted);font:12px ui-monospace,Consolas,monospace;padding:0 10px}
a{color:var(--acc)}</style></head><body><main>
<h1>🛟 Trang cứu hộ AI Company</h1>
<p class="muted">Trang này chạy riêng, vẫn vào được khi hệ thống chính đang deploy, khởi động lại hay bị lỗi. <a href="/">← Về trang chính</a></p>
<section class="card"><h2>Tình trạng</h2><div id="st" class="muted">Đang kiểm tra…</div>
<div class="row" style="margin-top:10px"><button id="re">🔄 Khởi động lại</button><button id="rb" class="d">⏪ Quay về bản trước</button></div>
<details style="margin-top:8px"><summary class="muted">Log của hệ thống chính</summary><pre id="logs"></pre></details>
<details><summary class="muted">Commit gần nhất</summary><pre id="commits"></pre></details></section>
<section class="card"><h2>Nhắn Khôi (bảo trì)</h2><div id="chat" class="log"></div>
<div class="row" style="margin-top:10px"><textarea id="t" rows="3" class="grow" placeholder="Ví dụ: hệ thống không vào được, xem log và sửa giúp anh"></textarea></div>
<div class="row" style="margin-top:8px"><button id="send" class="p">Gửi</button><button id="stop">⏹ Dừng</button><span id="busy" class="muted"></span></div></section>
<script>
const H={"Content-Type":"application/json","X-Rescue":"1"};
const post=(u,b)=>fetch("api/"+u,{method:"POST",headers:H,body:JSON.stringify(b||{})}).then(r=>r.json());
const esc=s=>s.replace(/[&<>]/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;"}[c]));
async function refresh(){try{const s=await fetch("api/status").then(r=>r.json());
const a=s.active==="active";document.getElementById("st").innerHTML=
`Dịch vụ: <b class="${a?"ok":"bad"}">${esc(s.active)}</b> từ ${esc(s.since||"—")}<br>Web chính: <b class="${s.web.startsWith("trả lời")?"ok":"bad"}">${esc(s.web)}</b><br><span class="muted">Bản đang chạy: ${s.binary} · bản trước: ${s.prev}</span>`;
document.getElementById("logs").textContent=s.logs||"(không đọc được log)";document.getElementById("commits").textContent=s.commits;
document.getElementById("busy").textContent=s.busy?"⏳ Khôi đang làm…":"";}catch(e){document.getElementById("st").textContent="Không đọc được tình trạng: "+e}}
let n=-1;async function chat(){const log=await fetch("api/chat").then(r=>r.json());if(log.length===n)return;n=log.length;const el=document.getElementById("chat");
el.innerHTML=log.map(m=>`<div class="m ${m.who}">${esc(m.text)}</div>`).join("")||'<p class="muted">Chưa có tin nhắn.</p>';el.scrollTop=el.scrollHeight}
document.getElementById("send").onclick=async()=>{const t=document.getElementById("t");if(!t.value.trim())return;const r=await post("chat",{text:t.value});if(r.error)alert(r.error);else t.value="";chat()};
document.getElementById("stop").onclick=async()=>{const r=await post("stop");alert(r.message)};
document.getElementById("re").onclick=async()=>{if(!confirm("Khởi động lại hệ thống chính?"))return;const r=await post("restart");alert(r.message);refresh()};
document.getElementById("rb").onclick=async()=>{if(!confirm("Quay về bản trước (bản đang chạy được giữ lại thành .bad)?"))return;const r=await post("rollback");alert(r.message);refresh()};
refresh();chat();setInterval(refresh,6000);setInterval(chat,2500);
</script></main></body></html>"""

LOGIN = r"""<!doctype html><html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Cứu hộ AI Company</title>
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;background:#10131c;color:#e8ebf2;font:15px system-ui,sans-serif}
form{background:#1a1f2d;border:1px solid #2c3448;border-radius:12px;padding:20px;width:min(340px,calc(100vw - 32px));display:grid;gap:10px}
input,button{font:inherit;padding:10px;border-radius:8px;border:1px solid #2c3448;background:#0b0e15;color:#e8ebf2}button{background:#5b8cff;border:0}
.e{color:#ff6b6b}</style></head><body><form method="post" action="login"><b>🛟 Cứu hộ AI Company</b>
<span style="color:#8f99b0">Đăng nhập bằng tài khoản AI Company</span>ERR
<input name="u" placeholder="Tài khoản" autocomplete="username" required><input name="p" type="password" placeholder="Mật khẩu" autocomplete="current-password" required>
<button>Vào</button></form></body></html>"""


class Handler(BaseHTTPRequestHandler):
    server_version = "rescue"

    def log_message(self, *a):  # quiet
        pass

    def path_only(self):
        p = self.path.split("?")[0]
        return p[len("/rescue"):] if p.startswith("/rescue") else p

    def authed(self):
        c = cookies.SimpleCookie(self.headers.get("Cookie", ""))
        return "rescue" in c and valid_token(c["rescue"].value)

    def send(self, code, body, ctype="application/json; charset=utf-8", headers=None):
        data = body.encode() if isinstance(body, str) else body
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Frame-Options", "DENY")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def j(self, obj, code=200):
        self.send(code, json.dumps(obj, ensure_ascii=False))

    def do_GET(self):
        p = self.path_only()
        if p in ("", "/"):
            if p == "":
                return self.send(302, "", headers={"Location": "/rescue/"})
            return self.send(200, PAGE if self.authed() else LOGIN.replace("ERR", ""), "text/html; charset=utf-8")
        if not self.authed():
            return self.j({"error": "Chưa đăng nhập"}, 401)
        if p == "/api/status":
            return self.j(status())
        if p == "/api/chat":
            return self.j(load_chat())
        self.j({"error": "Không có"}, 404)

    def do_POST(self):
        p = self.path_only()
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
        try:
            data = json.loads(body or "{}")
        except ValueError:
            data = {}
        if p == "/api/restart":
            ok, msg = restart()
            return self.j({"ok": ok, "message": msg})
        if p == "/api/rollback":
            try:
                ok, msg = rollback()
            except Exception as e:  # noqa: BLE001
                ok, msg = False, f"Lỗi: {e}"
            return self.j({"ok": ok, "message": msg})
        if p == "/api/stop":
            proc = RUN["proc"]
            pid = busy_pid()
            if proc:
                proc.kill()
            elif pid:
                try:
                    os.kill(pid, 9)
                except OSError:
                    pass
            return self.j({"message": "Đã dừng Khôi." if (proc or pid) else "Khôi không làm việc gì."})
        if p == "/api/chat":
            text = str(data.get("text", "")).strip()[:8000]
            if not text:
                return self.j({"error": "Tin nhắn trống"}, 400)
            if busy_pid() or RUN["proc"]:
                return self.j({"error": "Khôi đang làm việc khác (ở đây hoặc trên trang chính). Đợi xong hoặc bấm Dừng."}, 409)
            add_chat("me", text)
            threading.Thread(target=run_claude, args=(text,), daemon=True).start()
            return self.j({"ok": True})
        self.j({"error": "Không có"}, 404)


if __name__ == "__main__":
    os.makedirs(DATA, exist_ok=True)
    print(f"Rescue page on 127.0.0.1:{PORT}", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
