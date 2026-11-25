const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const extensionRoot = path.resolve(__dirname, "..");
const repositoryRoot = path.resolve(extensionRoot, "../..");
const executableName = process.platform === "win32" ? "rake-lsp.exe" : "rake-lsp";
const build = spawnSync(
  "cargo",
  [
    "build",
    "--manifest-path",
    path.join(repositoryRoot, "Cargo.toml"),
    "--release",
    "--bin",
    "rake-lsp"
  ],
  { cwd: repositoryRoot, stdio: "inherit" }
);

if (build.error) {
  throw build.error;
}
if (build.status !== 0) {
  process.exit(build.status || 1);
}

const builtExecutable = path.join(repositoryRoot, "target", "release", executableName);
const serverDirectory = path.join(extensionRoot, "server");
const bundledExecutable = path.join(serverDirectory, executableName);
fs.mkdirSync(serverDirectory, { recursive: true });
fs.copyFileSync(builtExecutable, bundledExecutable);
if (process.platform !== "win32") {
  fs.chmodSync(bundledExecutable, 0o755);
}
console.log(`Bundled ${path.relative(extensionRoot, bundledExecutable)}`);
