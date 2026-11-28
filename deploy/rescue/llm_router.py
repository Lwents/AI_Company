"""A small model router for the maintainer's Claude CLI, which only speaks the Anthropic Messages API.

- Models "opencode/…" (DeepSeek…) go to the Xoay proxy, which only speaks OpenAI Chat Completions:
  requests and (streamed) answers are translated both ways, tool calls included.
- Every other model is passed through unchanged to 9router (which speaks /v1/messages itself).

Callers must send the company's proxy key (Authorization: Bearer / x-api-key) — the same key 9router expects.
Standard library only; used by rescue.py at /llm/v1/… (and /rescue/llm/v1/… through nginx).
"""
import hmac
import json
import os
import ssl
import time
import urllib.error
import urllib.request
import uuid

NINEROUTER = os.environ.get("NINEROUTER_URL", "http://127.0.0.1:20128").rstrip("/")
XOAY = os.environ.get("AICO_XOAY_URL", "https://160.250.180.133/xoay/v1").rstrip("/")
XOAY_PREFIX = "opencode/"
TIMEOUT = 600
# Xoay sometimes never answers (seen with tool definitions): fail clearly instead of hanging the CLI.
XOAY_TIMEOUT = int(os.environ.get("AICO_XOAY_TIMEOUT", "120"))


def router_key():
    return os.environ.get("AICO_CLAUDE_KEY") or os.environ.get("CUSTOM_API_KEY", "")


def authorized(headers):
    want = router_key()
    if not want:
        return False
    got = headers.get("x-api-key") or ""
    auth = headers.get("Authorization") or ""
    if auth.lower().startswith("bearer "):
        got = auth[7:]
    return hmac.compare_digest(got.strip().encode(), want.encode())


# ---------------------------------------------------------------- Anthropic → OpenAI
def _text_of(content):
    if isinstance(content, str):
        return content
    out = []
    for b in content or []:
        if isinstance(b, dict) and b.get("type") == "text":
            out.append(b.get("text", ""))
        elif isinstance(b, str):
            out.append(b)
    return "\n".join(out)


def to_openai(req):
    msgs = []
    system = req.get("system")
    if system:
        msgs.append({"role": "system", "content": _text_of(system)})
    for m in req.get("messages", []):
        role, content = m.get("role"), m.get("content")
        if isinstance(content, str):
            msgs.append({"role": role, "content": content})
            continue
        if role == "assistant":
            text, calls = [], []
            for b in content or []:
                t = b.get("type")
                if t == "text":
                    text.append(b.get("text", ""))
                elif t == "tool_use":
                    calls.append({"id": b.get("id"), "type": "function", "function": {"name": b.get("name"), "arguments": json.dumps(b.get("input") or {}, ensure_ascii=False)}})
            msg = {"role": "assistant", "content": "\n".join(text) or None}
            if calls:
                msg["tool_calls"] = calls
            msgs.append(msg)
            continue
        # user: tool results become "tool" messages; text / images stay in one user message
        parts = []
        for b in content or []:
            t = b.get("type")
            if t == "tool_result":
                c = b.get("content")
                txt = _text_of(c) if not isinstance(c, str) else c
                if b.get("is_error"):
                    txt = "[error] " + txt
                msgs.append({"role": "tool", "tool_call_id": b.get("tool_use_id"), "content": txt or "(empty)"})
            elif t == "text":
                parts.append({"type": "text", "text": b.get("text", "")})
            elif t == "image":
                src = b.get("source") or {}
                if src.get("type") == "base64":
                    parts.append({"type": "image_url", "image_url": {"url": f"data:{src.get('media_type')};base64,{src.get('data')}"}})
        if parts:
            if all(p["type"] == "text" for p in parts):
                msgs.append({"role": "user", "content": "\n".join(p["text"] for p in parts)})
            else:
                msgs.append({"role": "user", "content": parts})
    out = {"model": req["model"], "messages": msgs, "max_tokens": req.get("max_tokens", 4096), "stream": bool(req.get("stream"))}
    if out["stream"]:
        out["stream_options"] = {"include_usage": True}
    if req.get("temperature") is not None:
        out["temperature"] = req["temperature"]
    tools = [t for t in req.get("tools") or [] if t.get("name") and t.get("input_schema")]
    if tools:
        out["tools"] = [{"type": "function", "function": {"name": t["name"], "description": t.get("description", ""), "parameters": t["input_schema"]}} for t in tools]
        tc = req.get("tool_choice") or {}
        if tc.get("type") == "any":
            out["tool_choice"] = "required"
        elif tc.get("type") == "tool":
            out["tool_choice"] = {"type": "function", "function": {"name": tc.get("name")}}
        elif tc.get("type") == "none":
            out["tool_choice"] = "none"
    return out


STOP = {"stop": "end_turn", "length": "max_tokens", "tool_calls": "tool_use", "function_call": "tool_use", "content_filter": "end_turn"}


def from_openai(resp, model):
    choice = (resp.get("choices") or [{}])[0]
    msg = choice.get("message") or {}
    content = []
    if msg.get("content"):
        content.append({"type": "text", "text": msg["content"]})
    for c in msg.get("tool_calls") or []:
        fn = c.get("function") or {}
        try:
            args = json.loads(fn.get("arguments") or "{}")
        except ValueError:
            args = {"_raw": fn.get("arguments")}
        content.append({"type": "tool_use", "id": c.get("id") or f"toolu_{uuid.uuid4().hex[:20]}", "name": fn.get("name"), "input": args})
    usage = resp.get("usage") or {}
    return {
        "id": f"msg_{uuid.uuid4().hex[:24]}",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content or [{"type": "text", "text": ""}],
        "stop_reason": STOP.get(choice.get("finish_reason"), "end_turn"),
        "stop_sequence": None,
        "usage": {"input_tokens": usage.get("prompt_tokens", 0), "output_tokens": usage.get("completion_tokens", 0)},
    }


# ---------------------------------------------------------------- HTTP plumbing
def _ctx():
    return ssl.create_default_context()


def _xoay_key():
    return os.environ.get("AICO_XOAY_KEY", "")


def handle(h, path, body):
    """Serves one /v1/… call for a BaseHTTPRequestHandler `h` (path is "/v1/…")."""
    if not authorized(h.headers):
        return _json(h, 401, {"type": "error", "error": {"type": "authentication_error", "message": "invalid key"}})
    try:
        req = json.loads(body or b"{}")
    except ValueError:
        req = {}
    model = str(req.get("model", ""))
    if path.startswith("/v1/messages/count_tokens"):
        if model.startswith(XOAY_PREFIX):
            # Rough estimate (≈4 characters per token) — the CLI only uses it to manage its context.
            return _json(h, 200, {"input_tokens": max(1, len(body or b"") // 4)})
        return _pass(h, path, body)
    if path.startswith("/v1/messages") and model.startswith(XOAY_PREFIX):
        return _xoay(h, req, model)
    return _pass(h, path, body)


def _json(h, code, obj):
    data = json.dumps(obj, ensure_ascii=False).encode()
    h.send_response(code)
    h.send_header("Content-Type", "application/json")
    h.send_header("Content-Length", str(len(data)))
    h.end_headers()
    h.wfile.write(data)


def _stream_headers(h, code, ctype):
    h.send_response(code)
    h.send_header("Content-Type", ctype)
    h.send_header("Cache-Control", "no-cache")
    h.send_header("X-Accel-Buffering", "no")  # nginx: do not hold the stream back
    h.send_header("Connection", "close")
    h.end_headers()
    h.close_connection = True


def _pass(h, path, body):
    """Unchanged to 9router (it speaks the Anthropic API for its own models)."""
    hdrs = {k: v for k, v in h.headers.items() if k.lower() in ("content-type", "anthropic-version", "anthropic-beta", "authorization", "x-api-key", "accept")}
    req = urllib.request.Request(NINEROUTER + path, data=body if body else None, headers=hdrs, method=h.command)
    try:
        r = urllib.request.urlopen(req, timeout=TIMEOUT)
    except urllib.error.HTTPError as e:
        data = e.read()
        h.send_response(e.code)
        h.send_header("Content-Type", e.headers.get("Content-Type", "application/json"))
        h.send_header("Content-Length", str(len(data)))
        h.end_headers()
        h.wfile.write(data)
        return
    except Exception as e:  # noqa: BLE001
        return _json(h, 502, {"type": "error", "error": {"type": "api_error", "message": f"9router: {e}"}})
    _stream_headers(h, r.status, r.headers.get("Content-Type", "application/json"))
    while True:
        chunk = r.read1(8192) if hasattr(r, "read1") else r.read(8192)
        if not chunk:
            break
        h.wfile.write(chunk)
        h.wfile.flush()


def _sse(h, event, data):
    h.wfile.write(f"event: {event}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n".encode())
    h.wfile.flush()


def _xoay(h, req, model):
    body = json.dumps(to_openai(req), ensure_ascii=False).encode()
    up = urllib.request.Request(XOAY + "/chat/completions", data=body, headers={"Content-Type": "application/json", "Authorization": "Bearer " + _xoay_key()})
    try:
        r = urllib.request.urlopen(up, timeout=XOAY_TIMEOUT, context=_ctx())
    except TimeoutError:
        return _json(h, 504, {"type": "error", "error": {"type": "api_error", "message": f"Xoay không trả lời sau {XOAY_TIMEOUT} giây (DeepSeek qua Xoay đang treo khi yêu cầu có kèm công cụ). Dùng /model chọn Neon."}})
    except urllib.error.HTTPError as e:
        detail = e.read()[:500].decode("utf-8", "replace")
        kind = "rate_limit_error" if e.code == 429 else "api_error"
        return _json(h, e.code if e.code in (400, 401, 403, 404, 429) else 502, {"type": "error", "error": {"type": kind, "message": f"Xoay: {detail}"}})
    except Exception as e:  # noqa: BLE001
        return _json(h, 502, {"type": "error", "error": {"type": "api_error", "message": f"Xoay: {e}"}})
    if not req.get("stream"):
        return _json(h, 200, from_openai(json.loads(r.read() or b"{}"), model))

    # Streaming: OpenAI chunks → Anthropic events.
    _stream_headers(h, 200, "text/event-stream")
    mid = f"msg_{uuid.uuid4().hex[:24]}"
    _sse(h, "message_start", {"type": "message_start", "message": {"id": mid, "type": "message", "role": "assistant", "model": model, "content": [], "stop_reason": None, "stop_sequence": None, "usage": {"input_tokens": 0, "output_tokens": 0}}})
    idx = -1
    text_open = False
    tools = {}  # openai tool index → anthropic block index
    open_tool = None
    stop, usage = "end_turn", {}
    for raw in r:
        line = raw.decode("utf-8", "replace").strip()
        if not line.startswith("data:"):
            continue
        data = line[5:].strip()
        if data == "[DONE]":
            break
        try:
            ch = json.loads(data)
        except ValueError:
            continue
        if ch.get("usage"):
            usage = ch["usage"]
        for choice in ch.get("choices") or []:
            d = choice.get("delta") or {}
            if d.get("content"):
                if open_tool is not None:
                    _sse(h, "content_block_stop", {"type": "content_block_stop", "index": open_tool})
                    open_tool = None
                if not text_open:
                    idx += 1
                    text_open = True
                    _sse(h, "content_block_start", {"type": "content_block_start", "index": idx, "content_block": {"type": "text", "text": ""}})
                _sse(h, "content_block_delta", {"type": "content_block_delta", "index": idx, "delta": {"type": "text_delta", "text": d["content"]}})
            for tc in d.get("tool_calls") or []:
                ti = tc.get("index", 0)
                fn = tc.get("function") or {}
                if ti not in tools:
                    if text_open:
                        _sse(h, "content_block_stop", {"type": "content_block_stop", "index": idx})
                        text_open = False
                    if open_tool is not None:  # blocks are sequential: close the previous call first
                        _sse(h, "content_block_stop", {"type": "content_block_stop", "index": open_tool})
                    idx += 1
                    open_tool = idx
                    tools[ti] = idx
                    _sse(h, "content_block_start", {"type": "content_block_start", "index": idx, "content_block": {"type": "tool_use", "id": tc.get("id") or f"toolu_{uuid.uuid4().hex[:20]}", "name": fn.get("name", ""), "input": {}}})
                if fn.get("arguments") and tools[ti] == open_tool:
                    _sse(h, "content_block_delta", {"type": "content_block_delta", "index": tools[ti], "delta": {"type": "input_json_delta", "partial_json": fn["arguments"]}})
            if choice.get("finish_reason"):
                stop = STOP.get(choice["finish_reason"], "end_turn")
    if text_open:
        _sse(h, "content_block_stop", {"type": "content_block_stop", "index": idx})
    if open_tool is not None:
        _sse(h, "content_block_stop", {"type": "content_block_stop", "index": open_tool})
    if tools and stop == "end_turn":
        stop = "tool_use"
    _sse(h, "message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None}, "usage": {"input_tokens": usage.get("prompt_tokens", 0), "output_tokens": usage.get("completion_tokens", 0)}})
    _sse(h, "message_stop", {"type": "message_stop"})
