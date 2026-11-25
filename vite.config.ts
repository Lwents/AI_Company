import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const backend = "http://127.0.0.1:8787";

export default defineConfig({
  root: "web",
  plugins: [react()],
  resolve: {
    // `@/features/...` instead of long relative paths.
    // (Vite resolves "/src/..." from `root`, i.e. web/src.)
    alias: [{ find: /^@\//, replacement: "/src/" }],
  },
  build: { outDir: "dist", emptyOutDir: true },
  server: {
    port: 5173,
    proxy: {
      "/api": backend,
      "/preview": backend,
      "/portal": backend,
      "/ws": { target: backend, ws: true },
    },
  },
});
