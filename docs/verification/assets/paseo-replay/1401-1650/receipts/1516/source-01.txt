import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const resourcesRoot = path.join(desktopRoot, "src-tauri", "resources", "managed-runtime");

const desktopPackageJson = JSON.parse(
  await fs.readFile(path.join(desktopRoot, "package.json"), "utf8")
);
const expectedVersion = desktopPackageJson.version;

const currentRuntime = JSON.parse(
  await fs.readFile(path.join(resourcesRoot, "current-runtime.json"), "utf8")
);
assert.equal(currentRuntime.runtimeVersion, expectedVersion, "current-runtime.json version mismatch");
assert.ok(currentRuntime.runtimeId, "current-runtime.json missing runtimeId");
assert.ok(currentRuntime.relativeRoot, "current-runtime.json missing relativeRoot");

const runtimeRoot = path.join(resourcesRoot, currentRuntime.relativeRoot);
const manifest = JSON.parse(
  await fs.readFile(path.join(runtimeRoot, "runtime-manifest.json"), "utf8")
);
assert.equal(manifest.runtimeId, currentRuntime.runtimeId, "manifest runtimeId mismatch");
assert.equal(manifest.runtimeVersion, expectedVersion, "manifest version mismatch");
assert.ok(manifest.nodeRelativePath, "manifest missing nodeRelativePath");
assert.ok(manifest.cliEntrypointRelativePath, "manifest missing cliEntrypointRelativePath");
assert.ok(manifest.serverRunnerRelativePath, "manifest missing serverRunnerRelativePath");

const nodeBinary = path.join(runtimeRoot, manifest.nodeRelativePath);
await fs.access(nodeBinary).catch(() => {
  throw new Error(`Bundled Node binary not found: ${nodeBinary}`);
});

const cliEntry = path.join(runtimeRoot, manifest.cliEntrypointRelativePath);
await fs.access(cliEntry).catch(() => {
  throw new Error(`CLI entrypoint not found: ${cliEntry}`);
});

const serverRunner = path.join(runtimeRoot, manifest.serverRunnerRelativePath);
await fs.access(serverRunner).catch(() => {
  throw new Error(`Server runner not found: ${serverRunner}`);
});

const runtimePackageJson = JSON.parse(
  await fs.readFile(path.join(runtimeRoot, "package.json"), "utf8")
);
assert.equal(runtimePackageJson.version, expectedVersion, "runtime package.json version mismatch");

for (const pkg of ["@getpaseo/relay", "@getpaseo/server", "@getpaseo/cli"]) {
  const pkgDir = path.join(runtimeRoot, "node_modules", ...pkg.split("/"));
  await fs.access(pkgDir).catch(() => {
    throw new Error(`Missing bundled dependency: ${pkg} (expected at ${pkgDir})`);
  });
}

console.log(`[validate-managed-runtime] PASS`);
console.log(`  runtimeId: ${manifest.runtimeId}`);
console.log(`  version: ${expectedVersion}`);
console.log(`  platform: ${manifest.platform}`);
console.log(`  arch: ${manifest.arch}`);
