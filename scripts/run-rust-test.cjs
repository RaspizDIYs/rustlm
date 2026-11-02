const { spawnSync } = require('node:child_process');
const { join } = require('node:path');
const [executable, ...args] = process.argv.slice(2);
// Tauri's app manifest is not linked into lib-test executables. TaskDialogIndirect needs Common Controls v6.
const manifest = spawnSync('mt.exe', ['-nologo', `-manifest`, join(__dirname, 'rust-test.manifest'),
  `-outputresource:${executable};#1`], { stdio: 'inherit', windowsHide: true });
if (manifest.error || manifest.status !== 0) {
  console.error('Windows SDK mt.exe is required on PATH to run native Tauri tests.');
  process.exit(1);
}
const result = spawnSync(executable, args, { stdio: 'inherit', windowsHide: true });
if (result.error) console.error(result.error.message);
process.exit(result.status ?? 1);
