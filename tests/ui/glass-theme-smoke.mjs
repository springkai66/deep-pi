// Start: pnpm exec vite --config tests/ui/vite.config.ts --port 1422
// Run: node tests/ui/glass-theme-smoke.mjs
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const browser = [
  process.env.CHROME_PATH,
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/usr/bin/chromium",
  "/usr/bin/google-chrome",
].find((path) => path && existsSync(path));
if (!browser) throw new Error("Set CHROME_PATH to a Chromium browser executable");

const temporaryRoot = process.platform === "win32" ? join(tmpdir(), "opencode") : tmpdir();
mkdirSync(temporaryRoot, { recursive: true });
const profile = mkdtempSync(join(temporaryRoot, "glass-theme-"));
try {
  const result = spawnSync(browser, [
    "--headless", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
    `--user-data-dir=${profile}`, "--dump-dom", "--virtual-time-budget=15000",
    process.argv[2] ?? "http://127.0.0.1:1422/glass-theme.html",
  ], { encoding: "utf8", timeout: 60000, maxBuffer: 10 * 1024 * 1024 });
  if (result.error) throw result.error;
  const verdict = result.stdout.match(/<pre id="glass-theme-result">([\s\S]*?)<\/pre>/)?.[1];
  console.log(verdict ?? "FAIL: Chromium did not return a completed glass-theme result");
  if (result.status !== 0 || !verdict?.startsWith("PASS ")) {
    console.error(`Chromium status=${result.status}, signal=${result.signal}`);
    if (result.stderr) console.error(result.stderr);
    process.exitCode = 1;
  }
} finally {
  rmSync(profile, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
}
