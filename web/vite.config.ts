import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

// Built here, outside kDrive; `make web` copies dist/ into crates/cs-app/web-dist,
// which the Rust binary embeds. In dev, /api is proxied to a running server.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: { proxy: { "/api": process.env.CS_API ?? "http://127.0.0.1:7878" } },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false },
  test: { environment: "jsdom", globals: true },
})
