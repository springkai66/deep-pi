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
const config = parseConfig(
  template
    .replaceAll("__DEEPPI_UPDATER_PUBLIC_KEY__", publicKey)
    .replaceAll("__DEEPPI_UPDATER_ENDPOINT__", endpoint),
);
writeFileSync(outputPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");
console.log(`wrote signed updater config: ${outputPath}`);
