/** JSON call to the local backend. Throws with the server's message on non-2xx. */
export async function api<T = any>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: body !== undefined ? { "content-type": "application/json" } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  if (res.status === 401) {
    // Session expired on a server with login: go sign in again.
    location.href = `/login?next=${encodeURIComponent(location.pathname)}`;
  }
  if (!res.ok) throw new Error(text || res.statusText);
  return text ? JSON.parse(text) : (undefined as T);
}

export async function uploadChatFiles(files: File[]): Promise<string[]> {
  const paths: string[] = [];
  for (const file of files) {
    const fn = (file.name || "")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/[^a-zA-Z0-9._-]+/g, "_")
      .replace(/^[._]+/, "")
      .slice(-80);
    const headers: Record<string, string> = {
      "Content-Type": file.type || "application/octet-stream",
    };
    if (fn) headers["X-Filename"] = fn;
    const res = await fetch("/api/maintainer/upload", {
      method: "POST",
      headers,
      body: file,
    });
    if (res.status === 401) {
      location.href = `/login?next=${encodeURIComponent(location.pathname)}`;
    }
    if (!res.ok) {
      const msg = await res.text().catch(() => "");
      throw new Error(msg || `Không tải tệp ${file.name} lên được`);
    }
    const data = (await res.json()) as { path?: string };
    if (data.path) paths.push(data.path);
  }
  return paths;
}
