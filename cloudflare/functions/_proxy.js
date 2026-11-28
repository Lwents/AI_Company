// Forwards a request to the AI Company backend on the VPS, unchanged (cookies, WebSocket upgrades,
// redirects). The browser only ever talks to this *.pages.dev origin, so the login cookie stays
// first-party. BACKEND_ORIGIN is set in wrangler.toml.
export async function proxy({ request, env }) {
  const url = new URL(request.url);
  const target = new URL(url.pathname + url.search, env.BACKEND_ORIGIN);
  const upstream = new Request(target, request);
  // Lets the backend tell proxied browser traffic from its own local callers.
  upstream.headers.set("X-Forwarded-Host", url.host);
  let res;
  try {
    res = await fetch(upstream, { redirect: "manual" });
  } catch (e) {
    res = null;
  }
  // The main app is restarting / deploying / broken: say so, and point to the rescue page (its own service).
  if (!url.pathname.startsWith("/rescue") && (!res || [502, 503, 504, 521, 522, 523, 524].includes(res.status))) {
    const wantsPage = request.method === "GET" && (request.headers.get("Accept") || "").includes("text/html");
    if (wantsPage) return new Response(DOWN_PAGE, { status: 503, headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store", "Retry-After": "10" } });
    return new Response(JSON.stringify({ error: "Máy chủ đang khởi động lại hoặc gặp sự cố. Trang cứu hộ: /rescue/" }), { status: 503, headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store" } });
  }
  return res;
}

const DOWN_PAGE = `<!doctype html><html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="refresh" content="15"><title>AI Company đang khởi động lại</title>
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;background:#10131c;color:#e8ebf2;font:16px system-ui,sans-serif;text-align:center;padding:16px}
a{display:inline-block;margin-top:14px;background:#5b8cff;color:#fff;padding:12px 18px;border-radius:10px;text-decoration:none}</style></head>
<body><div><h2>⏳ AI Company đang khởi động lại hoặc gặp sự cố</h2><p>Trang tự tải lại sau 15 giây.</p>
<a href="/rescue/">🛟 Vào trang cứu hộ</a><p style="color:#8f99b0;font-size:14px">Ở trang cứu hộ: xem lỗi, khởi động lại, quay về bản trước, nhắn Khôi sửa.</p></div></body></html>`;
