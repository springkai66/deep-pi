import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const { scripts } = JSON.parse(readFileSync("package.json", "utf8"));
const { build } = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));

describe("desktop development configuration invariants", () => {
  it("starts desktop development through the Tauri CLI", () => {
    expect(scripts["dev:desktop"]).toBe("tauri dev");
    expect(scripts.tauri).toBe("tauri");
  });

  it("keeps the beforeDevCommand frontend-only to prevent recursive Tauri launches", () => {
    expect(build.beforeDevCommand).toBe("pnpm dev");
    expect(scripts.dev).toBe("vite dev");
    expect(build.devUrl).toBe("http://localhost:1420");
  });
});
