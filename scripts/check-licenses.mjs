import { execFileSync } from "node:child_process";
import process from "node:process";

const root = new URL("..", import.meta.url);
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const allowedNpmLicenses = new Set([
  "MIT",
  "ISC",
  "Apache-2.0",
  "Apache-2.0 OR MIT",
  "MIT OR Apache-2.0",
]);

function run(executable, args) {
  if (process.platform === "win32" && executable === "pnpm") {
    return execFileSync(process.env.ComSpec ?? "cmd.exe", [
      "/d",
      "/s",
      "/c",
      "pnpm licenses list --prod --json",
    ], {
      cwd: root,
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
      stdio: ["ignore", "pipe", "pipe"],
    });
  }
  return execFileSync(executable, args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function fail(message) {
  console.error(`license check failed: ${message}`);
  process.exitCode = 1;
}

try {
  const npmReport = JSON.parse(
    run(process.platform === "win32" ? "pnpm" : "pnpm", ["licenses", "list", "--prod", "--json"]),
  );
  const unexpectedNpm = Object.keys(npmReport).filter((license) => !allowedNpmLicenses.has(license));
  if (unexpectedNpm.length > 0) {
    fail(`unapproved npm licenses: ${unexpectedNpm.join(", ")}`);
  } else {
    console.log(`npm production licenses: ${Object.keys(npmReport).length} approved groups`);
  }
} catch (error) {
  fail(`could not read npm licenses: ${error instanceof Error ? error.message : String(error)}`);
}

try {
  const metadata = JSON.parse(
    run(cargo, ["metadata", "--manifest-path", "src-tauri/Cargo.toml", "--format-version", "1", "--locked"]),
  );
  const missing = metadata.packages
    .filter((pkg) => !pkg.license)
    .map((pkg) => pkg.name);
  if (missing.length > 0) {
    fail(`Cargo packages without a declared license: ${missing.join(", ")}`);
  } else {
    console.log(`Cargo package licenses: ${metadata.packages.length} declared`);
  }
} catch (error) {
  fail(`could not read Cargo licenses: ${error instanceof Error ? error.message : String(error)}`);
}

if (process.exitCode === 0 || process.exitCode === undefined) {
  console.log("license check: PASS");
}
