import { defineConfig } from "vite";

const proxy = {
  "/api": "http://127.0.0.1:8080",
  "/health": "http://127.0.0.1:8080",
};
export default defineConfig({ server: { proxy }, preview: { proxy } });
