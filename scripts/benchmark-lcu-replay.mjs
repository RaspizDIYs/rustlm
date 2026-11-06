import { createReplay, replayPassed } from './lcu-replay.mjs';
import { writeFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';

// Local synthetic stress probe; it never starts LM or contacts Riot.
const count = Number(process.argv[2] || 50000);
if (!Number.isSafeInteger(count) || count < 1 || count > 100000) throw new Error('Count must be 1..100000');
const route = '/lol-champ-select/v1/session';
global.gc?.();
const before = process.memoryUsage();
const cpu = process.cpuUsage();
const started = performance.now();
const entries = Array.from({length:count}, (_, index) => ({
  kind:'Update', method:'WS', route, at_ms:0, data:{gameId:index+1},
}));
const replay = await createReplay(entries);
let final;
let reads = 0;
let peak = process.memoryUsage().rss;
try {
  const headers = {Authorization:`Basic ${Buffer.from(`riot:${replay.password}`).toString('base64')}`};
  const deadline = performance.now()+10000;
  do {
    const response = await fetch(`http://127.0.0.1:${replay.port}${route}`,{headers});
    final = await response.json(); reads++;
    peak = Math.max(peak,process.memoryUsage().rss);
    if (performance.now()>deadline) throw new Error('Replay did not drain before deadline');
  } while (final.gameId !== count);
  if (!replayPassed(replay.report()) || replay.actions.length !== 0) throw new Error('Unexpected replay mutation');
} finally { await replay.close(); }
const report = {
  format:'rustlm-local-replay-benchmark',version:1,node:process.version,platform:process.platform,
  created_at:new Date().toISOString(),events:count,reads,elapsed_ms:Math.round(performance.now()-started),
  cpu_ms:Object.fromEntries(Object.entries(process.cpuUsage(cpu)).map(([k,v])=>[k,v/1000])),
  baseline_rss_bytes:before.rss,sampled_peak_rss_bytes:peak,rss_growth_bytes:peak-before.rss,
  replay:replay.report(),
  scope:'Synthetic Node replay including fixture allocation and loopback HTTP; sampled RSS is not whole-LM peak memory',
};
await mkdir('artifacts',{recursive:true});
const target=resolve('artifacts/lcu-replay-benchmark.json');
await writeFile(target,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report));
