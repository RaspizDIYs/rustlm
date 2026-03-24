const { spawnSync } = require("child_process");
const { existsSync } = require("fs");
const { join } = require("path");

const ext = process.platform === "win32" ? ".exe" : "";
const dir = join(__dirname, "..", "src-tauri", "target", "release");
const names = [`rustlm${ext}`, `RustLM${ext}`];
const exe = names.map((n) => join(dir, n)).find((p) => existsSync(p));

if (!exe) {
  console.error("Нет exe в src-tauri/target/release/. Сначала: npm run tauri:build:test");
  process.exit(1);
}

const r = spawnSync(exe, { stdio: "inherit" });
process.exit(r.status ?? 1);
