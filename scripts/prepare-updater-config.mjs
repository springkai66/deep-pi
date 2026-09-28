import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const templatePath = join(root, "src-tauri", "tauri.release.conf.json");
const outputPath = join(root, "src-tauri", "tauri.release.generated.conf.json");
const publicKey = process.env.DEEPPI_UPDATER_PUBLIC_KEY?.trim();
const repository = process.env.GITHUB_REPOSITORY?.trim();
const endpoint =
  process.env.DEEPPI_UPDATER_ENDPOINT?.trim() ||
  (repository ? `https://github.com/${repository}/releases/download/stable/latest.json` : "");

if (!publicKey || publicKey === "__DEEPPI_UPDATER_PUBLIC_KEY__") {
  throw new Error("DEEPPI_UPDATER_PUBLIC_KEY is required for a signed updater build.");
}
if (!endpoint || endpoint === "__DEEPPI_UPDATER_ENDPOINT__") {
  throw new Error(
    "DEEPPI_UPDATER_ENDPOINT or GITHUB_REPOSITORY is required for a signed updater build.",
  );
}

function parseHttpsUrl(value) {
  try {
    return new URL(value);
  } catch {
    throw new Error("DEEPPI_UPDATER_ENDPOINT must be a valid URL.");
  }
}

function parseConfig(value) {
  try {
    return JSON.parse(value);
  } catch {
    throw new Error("tauri.release.conf.json is not valid JSON.");
  }
}

const url = parseHttpsUrl(endpoint);
if (url.protocol !== "https:" || url.username || url.password) {
  throw new Error("DEEPPI_UPDATER_ENDPOINT must be an HTTPS URL without credentials.");
}

const template = readFileSync(templatePath, "utf8");
const rendered = template
  .replaceAll("__DEEPPI_UPDATER_PUBLIC_KEY__", publicKey)
  .replaceAll("__DEEPPI_UPDATER_ENDPOINT__", endpoint);
// 模板里若有新增占位符而这里忘了替换，出来的配置会带着 __...__ 字样直接打进安装包，
// updater 会静默失效；因此不把「没找到就跳过」当作正常情况。
const leftover = rendered.match(/__[A-Z0-9_]+__/g);
if (leftover) {
  throw new Error(
    `tauri.release.conf.json still contains unresolved placeholders: ${leftover.join(", ")}`,
  );
}
const config = parseConfig(rendered);
writeFileSync(outputPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");
console.log(`wrote signed updater config: ${outputPath}`);
