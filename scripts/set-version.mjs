// Stamps a release version into every file that carries one.
// Invoked by semantic-release (@semantic-release/exec prepareCmd) and by CI
// before building the bundles: node scripts/set-version.mjs <version>

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!version) {
  console.error("usage: node scripts/set-version.mjs <version>");
  process.exit(1);
}

// package.json + package-lock.json (npm keeps them in sync)
execSync(`npm version ${version} --no-git-tag-version --allow-same-version`, {
  cwd: "app",
  stdio: "inherit",
});

// tauri.conf.json — the version Tauri stamps into the bundles. Windows MSI
// (WiX) only accepts numeric major.minor.patch, so strip any semver
// prerelease/build suffix (e.g. 0.2.0-dev.1 -> 0.2.0). The full semver still
// lives in the git tag / GitHub release; this is only the installer version.
const conf = "app/src-tauri/tauri.conf.json";
const bundleVersion = version.split(/[-+]/)[0];
const json = JSON.parse(readFileSync(conf, "utf8"));
json.version = bundleVersion;
writeFileSync(conf, JSON.stringify(json, null, 2) + "\n");

console.log(
  bundleVersion === version
    ? `version set to ${version}`
    : `version set to ${version} (bundles use ${bundleVersion})`,
);
