import { spawn } from 'node:child_process';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createSimulator } from './lcu-simulator.mjs';
import { createReplay, parseRecording, replayPassed } from './lcu-replay.mjs';

const directory = await mkdtemp(join(tmpdir(), 'rustlm-lcu-'));
const successor = await createSimulator();
const simulator = await createSimulator({ successor: { port: successor.port, password: successor.password } });
try {
  const config = join(directory, 'connection.json');
  await writeFile(config, JSON.stringify({ port: simulator.port, password: simulator.password }));
  const child = spawn('cargo', ['test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--lib',
    process.argv[2] || 'simulator_', '--', '--ignored', '--skip', 'replay_ready_check_', '--test-threads=1'], {
    stdio: 'inherit', cwd: fileURLToPath(new URL('../', import.meta.url)),
    env: { ...process.env, RUSTLM_TEST_LCU: config, RUSTLM_RECORD_OUTPUT: join(directory, "recording.jsonl"),
      ...(process.platform === 'win32' ? {
        CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER: 'node ../scripts/run-rust-test.cjs',
      } : {}),
    },
  });
  process.exitCode = await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', code => resolve(code ?? 1));
  });
  if (process.exitCode === 0 && !process.argv[2]) {
    const replay = await createReplay(parseRecording(await readFile(join(directory,'recording.jsonl'),'utf8')));
    try {
      await writeFile(config, JSON.stringify({port:replay.port,password:replay.password}));
      const replayChild = spawn('cargo',['test','--locked','--manifest-path','src-tauri/Cargo.toml','--lib','replay_ready_check_','--','--ignored','--test-threads=1'],{
        stdio:'inherit',cwd:fileURLToPath(new URL('../',import.meta.url)),
        env:{...process.env,RUSTLM_TEST_LCU:config,RUSTLM_REPLAY_SMOKE:'1',
          ...(process.platform==='win32'?{CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER:'node ../scripts/run-rust-test.cjs'}:{})},
      });
      process.exitCode = await new Promise((resolve,reject)=>{replayChild.once('error',reject);replayChild.once('exit',code=>resolve(code??1));});
      const report = replay.report();
      if (!replayPassed(report)) throw new Error(`Replay failed: ${JSON.stringify(report)}`);
      console.log('Native recorded-session replay:', report);
    } finally {await replay.close();}
  }

} finally {
  await simulator.close();
  await successor.close();
  await rm(directory, { recursive: true, force: true });
}
