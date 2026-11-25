/** JSON call to the local backend. Throws with the server's message on non-2xx. */
export async function api<T = any>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: body !== undefined ? { "content-type": "application/json" } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  if (!res.ok) throw new Error(text || res.statusText);
  return text ? JSON.parse(text) : (undefined as T);
}
