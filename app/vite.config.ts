import { defineConfig } from "vite";

// Tauri хочет фиксированный порт; экран не чистим, чтобы не терять ошибки Rust.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Всё локально, никаких удалённых origin; шрифты и прочие ассеты кладём как есть.
  build: {
    target: "es2022",
    minify: "esbuild",
    sourcemap: false,
  },
});
