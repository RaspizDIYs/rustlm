import { spawn } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createSimulator } from './lcu-simulator.mjs';

const directory = await mkdtemp(join(tmpdir(), 'rustlm-lcu-'));
const successor = await createSimulator();
const simulator = await createSimulator({ successor: { port: successor.port, password: successor.password } });
try {
  const config = join(directory, 'connection.json');
  await writeFile(config, JSON.stringify({ port: simulator.port, password: simulator.password }));
  const child = spawn('cargo', ['test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib',
    process.argv[2] || 'simulator_', '--', '--ignored', '--test-threads=1'], {
    stdio: 'inherit', cwd: fileURLToPath(new URL('../', import.meta.url)),
    env: { ...process.env, RUSTLM_TEST_LCU: config,
      ...(process.platform === 'win32' ? {
        CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER: 'node ../scripts/run-rust-test.cjs',
      } : {}),
    },
  });
  process.exitCode = await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', code => resolve(code ?? 1));
  });
} finally {
  await simulator.close();
  await successor.close();
  await rm(directory, { recursive: true, force: true });
}
