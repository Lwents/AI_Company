/** Server facts loaded once before the app renders (login state, where previews are served). */
export interface Runtime {
  /** Login is required (server reachable from a network). */
  enabled: boolean;
  user: string | null;
  /** Origin serving project previews, e.g. "https://1.2.3.4:8444"; "" = same origin. */
  previewOrigin: string;
}

export const runtime: Runtime = { enabled: false, user: null, previewOrigin: "" };

export async function loadRuntime(): Promise<void> {
  try {
    const r = await fetch("/api/auth/status");
    if (r.ok) Object.assign(runtime, await r.json());
  } catch {
    // Offline / old server: keep the local defaults.
  }
  // UI served statically (e.g. Cloudflare Pages): nothing guards the page itself, so go sign in.
  if (runtime.enabled && !runtime.user) {
    location.replace(`/login?next=${encodeURIComponent(location.pathname)}`);
    await new Promise(() => {});
  }
}

/** Link to a project's preview. On a server it lives on its own origin, so generated code cannot call the admin API. */
export function previewUrl(projectId: string, path = ""): string {
  return `${runtime.previewOrigin}/preview/${projectId}/${path}`;
}

export async function logout(): Promise<void> {
  try {
    await fetch("/api/auth/logout", { method: "POST" });
  } finally {
    location.href = "/login";
  }
}
