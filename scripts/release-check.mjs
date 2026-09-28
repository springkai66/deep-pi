import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const errors = [];
const warnings = [];
const requireInstaller = process.argv.includes("--require-installer");
const requireUpdater = process.argv.includes("--require-updater");

function readJson(relativePath) {
  const path = join(root, relativePath);
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    errors.push(`${relativePath}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

function requireFile(relativePath) {
  if (!existsSync(join(root, relativePath))) errors.push(`missing required file: ${relativePath}`);
}

function listFiles(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? listFiles(path) : [path];
  });
}

const packageJson = readJson("package.json");
if (packageJson) {
  if (packageJson.license !== "MIT") errors.push("package.json must declare MIT license");
  if (packageJson.packageManager !== "pnpm@10.30.3") {
    errors.push("package.json must pin pnpm@10.30.3");
  }
  for (const script of ["check", "test", "build", "security:audit", "licenses:check", "release:check", "perf:smoke", "perf:soak", "installer:smoke", "updater:prepare", "updater:manifest"]) {
    if (!packageJson.scripts?.[script]) errors.push(`package.json is missing the ${script} script`);
  }
}

const cargoToml = readFileSync(join(root, "src-tauri", "Cargo.toml"), "utf8");
if (!/^license\s*=\s*["']MIT["']/m.test(cargoToml)) {
  errors.push("src-tauri/Cargo.toml must declare MIT license");
}

const tauri = readJson("src-tauri/tauri.conf.json");
if (tauri) {
  if (tauri.bundle?.active !== true) errors.push("Tauri bundling must be enabled");
  if (tauri.bundle?.windows?.webviewInstallMode?.type !== "downloadBootstrapper") {
    errors.push("Windows must use the WebView2 download bootstrapper");
  }
  if (!tauri.plugins?.updater || !Array.isArray(tauri.plugins.updater.endpoints)) {
    errors.push("Tauri updater base configuration is missing");
  }
}

// 版本号散在三个清单里；升版本时漏改其中一处，会让安装包与包元数据版本不一致
// （updater 会拿 package.json 的版本去比对 latest.json），所以在发布检查里直接卡住。
// `Cargo.lock` 也带版本：`cargo build` 会自动把它改成 Cargo.toml 的值，如果忘了提交，
// 下次 CI 会以脏工作区构建或产生额外 diff，因此一并校验。
const cargoVersion = /^version\s*=\s*["']([^"']+)["']/m.exec(cargoToml)?.[1];
const cargoLockPath = join(root, "src-tauri", "Cargo.lock");
// 注意：仓库里 Cargo.lock 是 CRLF（Windows 检出），所以不能用 `\n` 锚定行尾，
// 否则在本地能跑、在 CI（LF 检出）或反之会静默取不到版本。
const cargoLockVersion = existsSync(cargoLockPath)
  ? /\[\[package\]\]\r?\nname = "deeppi"\r?\nversion = "([^"]+)"/.exec(readFileSync(cargoLockPath, "utf8"))?.[1]
  : undefined;
const versionSources = {
  "package.json": packageJson?.version,
  "src-tauri/tauri.conf.json": tauri?.version,
  "src-tauri/Cargo.toml": cargoVersion,
  "src-tauri/Cargo.lock": cargoLockVersion,
};
const declaredVersions = new Set(Object.values(versionSources).filter(Boolean));
if (Object.values(versionSources).some((value) => !value)) {
  errors.push(
    `version is missing in: ${Object.entries(versionSources).filter(([, value]) => !value).map(([file]) => file).join(", ")}`,
  );
} else if (declaredVersions.size !== 1) {
  errors.push(
    `version mismatch across manifests: ${Object.entries(versionSources).map(([file, value]) => `${file}=${value}`).join(", ")}`,
  );
}

for (const required of [
  "README.md",
  "SECURITY.md",
  "TROUBLESHOOTING.md",
  "scripts/perf-smoke.ps1",
  "scripts/installer-smoke.ps1",
  "scripts/prepare-updater-config.mjs",
  "scripts/prepare-updater-manifest.mjs",
  "src-tauri/tauri.release.conf.json",
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
  ".github/workflows/rollback.yml",
]) {
  requireFile(required);
}

const releaseBinary = join(root, "src-tauri", "target", "release", "deeppi.exe");
if (existsSync(releaseBinary)) {
  console.log(`release binary: ${releaseBinary}`);
} else {
  warnings.push("release binary is not built; run cargo build --release or pnpm tauri build");
}

const bundleRoot = join(root, "src-tauri", "target", "release", "bundle");
const installers = listFiles(bundleRoot).filter((path) => /\.(msi|exe)$/i.test(path));
const nsisInstallers = listFiles(join(bundleRoot, "nsis")).filter((path) => /-setup\.exe$/i.test(path));
if (requireInstaller && installers.length === 0) {
  errors.push("no MSI or NSIS installer found under src-tauri/target/release/bundle");
}
const updaterFiles = listFiles(bundleRoot);
const hasUpdaterManifest = updaterFiles.some((path) => /[\\/]latest\.json$/i.test(path));
const hasUpdaterSignature = nsisInstallers.some((path) => existsSync(`${path}.sig`));
if (requireUpdater && !hasUpdaterManifest) {
  errors.push("no updater latest.json found under src-tauri/target/release/bundle");
}
if (requireUpdater && !hasUpdaterSignature) {
  errors.push("no NSIS updater signature found under src-tauri/target/release/bundle/nsis");
}
if (requireUpdater && hasUpdaterManifest) {
  try {
    const manifestPath = join(bundleRoot, "latest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    const windows = manifest.platforms?.["windows-x86_64"];
    if (!manifest.version || !windows?.signature || !windows?.url) {
      errors.push("updater latest.json is missing version, signature, or Windows URL");
    } else {
      if (packageJson?.version && manifest.version !== packageJson.version) {
        errors.push(`updater version ${manifest.version} does not match package.json ${packageJson.version}`);
      }
      let updaterUrl;
      try {
        updaterUrl = new URL(String(windows.url));
      } catch {
        errors.push("updater Windows URL is invalid");
      }
      if (updaterUrl && (updaterUrl.protocol !== "https:" || updaterUrl.username || updaterUrl.password)) {
        errors.push("updater Windows URL must be HTTPS without credentials");
      }
      const installer = nsisInstallers.find((path) => basename(path) === basename(updaterUrl?.pathname ?? ""));
      if (!installer) {
        errors.push("updater Windows URL does not point to a local NSIS installer");
      } else {
        const signaturePath = `${installer}.sig`;
        const localSignature = existsSync(signaturePath) ? readFileSync(signaturePath, "utf8").trim() : "";
        if (!localSignature || localSignature !== String(windows.signature).trim()) {
          errors.push(`updater signature does not match ${basename(installer)}`);
        }
      }
    }
  } catch (error) {
    errors.push(`latest.json: ${error instanceof Error ? error.message : String(error)}`);
  }
}
if (installers.length > 0) {
  console.log(`installers: ${installers.map((path) => path.replace(`${root}\\`, "")).join(", ")}`);
}

if (warnings.length > 0) {
  for (const warning of warnings) console.warn(`warning: ${warning}`);
}
if (errors.length > 0) {
  for (const error of errors) console.error(`error: ${error}`);
  process.exitCode = 1;
} else {
  console.log("release check: PASS");
}
