import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bundleRoot = join(root, "src-tauri", "target", "release", "bundle");
const nsisRoot = join(bundleRoot, "nsis");
const repository = process.env.GITHUB_REPOSITORY?.trim();
const endpoint =
  process.env.DEEPPI_UPDATER_ENDPOINT?.trim() ||
  (repository ? `https://github.com/${repository}/releases/download/stable/latest.json` : "");
function parseJson(value, label) {
  try {
    return JSON.parse(value);
  } catch {
    throw new Error(`${label} is not valid JSON.`);
  }
}

function parseHttpsUrl(value) {
  try {
    return new URL(value);
  } catch {
    throw new Error("DEEPPI_UPDATER_ENDPOINT must be a valid URL.");
  }
}

function resolveUrl(value, base) {
  try {
    return new URL(value, base).toString();
  } catch {
    throw new Error(`updater artifact URL is invalid: ${value}`);
  }
}

const version = parseJson(readFileSync(join(root, "package.json"), "utf8"), "package.json").version;

if (!endpoint) throw new Error("DEEPPI_UPDATER_ENDPOINT is required.");
const endpointUrl = parseHttpsUrl(endpoint);
if (endpointUrl.protocol !== "https:" || endpointUrl.username || endpointUrl.password) {
  throw new Error("DEEPPI_UPDATER_ENDPOINT must be an HTTPS URL without credentials.");
}

const installer = readdirSync(nsisRoot).find((name) => /-setup\.exe$/i.test(name));
if (!installer) throw new Error(`No NSIS installer found under ${nsisRoot}.`);
const signaturePath = join(nsisRoot, `${installer}.sig`);
if (!existsSync(signaturePath)) throw new Error(`No updater signature found for ${installer}.`);

const manifest = {
  version,
  notes: process.env.DEEPPI_RELEASE_NOTES?.trim() || `DeepPi ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(signaturePath, "utf8").trim(),
      url: resolveUrl(installer, endpointUrl),
    },
  },
};

const outputPath = join(bundleRoot, "latest.json");
writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
console.log(`wrote updater manifest: ${outputPath}`);
