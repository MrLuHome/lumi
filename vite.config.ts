import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 通过固定端口加载开发界面，端口被占用时直接失败，避免静默换端口导致白屏。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2022",
    sourcemap: false,
  },
});
