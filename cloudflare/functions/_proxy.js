// Forwards a request to the AI Company backend on the VPS, unchanged (cookies, WebSocket upgrades,
// redirects). The browser only ever talks to this *.pages.dev origin, so the login cookie stays
// first-party. BACKEND_ORIGIN is set in wrangler.toml.
export async function proxy({ request, env }) {
  const url = new URL(request.url);
  const target = new URL(url.pathname + url.search, env.BACKEND_ORIGIN);
  const upstream = new Request(target, request);
  // Lets the backend tell proxied browser traffic from its own local callers.
  upstream.headers.set("X-Forwarded-Host", url.host);
  return fetch(upstream, { redirect: "manual" });
}
