import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

// Built here, outside kDrive; `make web` copies dist/ into crates/cs-app/web-dist,
// which the Rust binary embeds. In dev, /api is proxied to a running server.
// The in-app guide is docs/guide of the repository, bundled at build time.
// `make web` builds from a copy of web/, so CS_GUIDE_DIR can point at the real folder.
const guideDir = process.env.CS_GUIDE_DIR ?? path.resolve(import.meta.dirname, "../docs/guide")

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(import.meta.dirname, "./src"), "@guide": guideDir } },
  // Port 5199 is the devUrl of the desktop shell (crates/cs-desktop/tauri.conf.json): strict, so a busy port is an error.
  server: { host: "127.0.0.1", port: 5199, strictPort: true, fs: { allow: [import.meta.dirname, guideDir] }, proxy: { "/api": process.env.CS_API ?? "http://127.0.0.1:7878" } },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false },
  test: { environment: "jsdom", globals: true, setupFiles: ["./src/test-setup.ts"] },
})
