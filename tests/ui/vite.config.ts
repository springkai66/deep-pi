import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { svelte, vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  plugins: [svelte({ configFile: false, preprocess: vitePreprocess() })],
  resolve: { alias: { $lib: fileURLToPath(new URL("../../src/lib", import.meta.url)) } },
  server: {
    host: "127.0.0.1", port: 1421, strictPort: true,
    fs: { allow: [fileURLToPath(new URL("../..", import.meta.url))] },
  },
});
