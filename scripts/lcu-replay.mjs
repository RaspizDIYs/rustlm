// Replay never contacts Riot. Only an explicitly configured debug LM can connect here.
import http from 'node:http';
import { readFile, writeFile, mkdir, rename, rm } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createInterface } from 'node:readline';
const { WebSocketServer } = createRequire(import.meta.url)('next/dist/compiled/ws');

export function replayPassed(report) {
  return report.expected === report.consumed && report.mismatches === 0;
}

export function parseRecording(text) {
  if (Buffer.byteLength(text) > 21 * 1024 * 1024) throw new Error('Recording is too large');
  const lines = text.trimEnd().split('\n');
  // A crashed application can leave only its final write incomplete; retain complete records.
  const values = lines.flatMap((line, index) => {
    try { return [JSON.parse(line)]; }
    catch (error) {
      if (index > 0 && index === lines.length - 1 && !text.endsWith('\n')) {
        console.warn('Incomplete final recording entry was skipped'); return [];
      }
      throw error;
    }
  });
  const [header, ...entries] = values;
  if (header?.format !== 'rustlm-lcu-session' || header.version !== 1) throw new Error('Unsupported recording');
  let last = 0;
  for (const e of entries) {
    if (!e || typeof e !== 'object' || Array.isArray(e)
      || !Number.isSafeInteger(e.at_ms) || e.at_ms < last || e.at_ms > 1801000
      || !/^\/lol-(gameflow|matchmaking|champ-select|perks|lobby-team-builder)(?:\/champ-select)?\/v1\/[a-z0-9/-]+$/.test(e.route)
      || !['http','Create','Update','Delete','Disconnect'].includes(e.kind)
      || !['GET','POST','PUT','PATCH','DELETE','WS'].includes(e.method)
      || (e.kind === 'http' ? e.method === 'WS' : e.method !== 'WS')
      || (e.request != null && (typeof e.request !== 'object' || Array.isArray(e.request)))
      || (e.kind === 'http' && (!Number.isInteger(e.status) || e.status < 100 || e.status > 599))) throw new Error('Invalid recording entry');
    last = e.at_ms;
  }
  return entries;
}

export async function createReplay(entries, { speed = 1 } = {}) {
  if (!Number.isFinite(speed) || speed < 0.1 || speed > 10) throw new Error('Speed must be 0.1..10');
  const password = randomUUID();
  const auth = `Basic ${Buffer.from(`riot:${password}`).toString('base64')}`;
  const ws = new WebSocketServer({ noServer: true, maxPayload: 1024 });
  const snapshots = new Map();
  const writes = entries.filter(e => e.kind === 'http' && e.method !== 'GET');
  const actions = [];
  let mismatches = 0;
  const recordAction = (method, route, matched) => {
    if (!matched) mismatches++;
    if (actions.length === 1000) actions.shift();
    actions.push({ method, route, matched });
  };
  let nextWrite = 0;
  let started = false;
  let startedAt = 0;
  let timer;
  let nextEvent = 0;
  let closed = false;
  let closePromise;
  const clients = new Set();
  const advance = e => {
    if (e.kind === 'Disconnect') { for (const client of clients) client.terminate(); return; }
    if (e.kind === 'http' && e.method === 'GET') snapshots.set(e.route, e);
    if (e.kind !== 'http') {
      snapshots.set(e.route, { ...e, status: e.kind === 'Delete' ? 404 : 200 });
      const frame = JSON.stringify([8, 'OnJsonApiEvent', { uri: e.route, eventType: e.kind, data: e.data }]);
      for (const client of clients) if (client.readyState === 1) client.send(frame);
    }
  };
  const schedule = () => {
    if (closed) return;
    const elapsed = (performance.now() - startedAt) * speed;
    // One timer and bounded batches keep large traces from starving HTTP handling.
    let batch = 0;
    while (nextEvent < entries.length && entries[nextEvent].at_ms <= elapsed && batch++ < 1000) {
      advance(entries[nextEvent++]);
    }
    if (nextEvent < entries.length) {
      timer = setTimeout(schedule, Math.max(0, entries[nextEvent].at_ms / speed - (performance.now() - startedAt)));
    }
  };
  const start = () => {
    if (started) return;
    started = true;
    startedAt = performance.now();
    // The first consumer starts the clock; opening the tool must not consume the trace.
    schedule();
  };
  const server = http.createServer(async (request, response) => {
    const reply = (status, data) => { response.writeHead(status, { 'Content-Type': 'application/json' }); response.end(status === 204 ? undefined : JSON.stringify(data ?? null)); };
    if (request.headers.authorization !== auth) return reply(401, {});
    start();
    const route = request.url;
    if (request.method === 'GET') {
      const snapshot = snapshots.get(route);
      return snapshot ? reply(snapshot.status, snapshot.data) : reply(404, {});
    }
    let body = '';
    try {
      for await (const chunk of request) { body += chunk; if (body.length > 8192) return reply(413, {}); }
    } catch { return; }
    let data;
    try { data = body ? JSON.parse(body) : null; } catch { return reply(400, {}); }
    const expected = writes[nextWrite];
    // A matching payload before its recorded time must not hide a premature action.
    if (expected && performance.now() - startedAt < expected.at_ms / speed) {
      recordAction(request.method, route, false);
      return reply(409, { errorCode: 'REPLAY_ACTION_TOO_EARLY' });
    }
    const matches = expected && expected.route === route && expected.method === request.method
      && Object.entries(expected.request || {}).every(([key, value]) => JSON.stringify(data?.[key]) === JSON.stringify(value));
    recordAction(request.method, route, Boolean(matches));
    if (!matches) return reply(409, { errorCode: 'REPLAY_ACTION_MISMATCH' });
    nextWrite++;
    return reply(expected.status, expected.data);
  });
  server.on('upgrade', (request, socket, head) => {
    if (request.headers.authorization !== auth) return socket.destroy();
    ws.handleUpgrade(request, socket, head, client => {
      client.on('error', () => {});
      client.on('message', message => {
        if (message.toString() === '[5,"OnJsonApiEvent"]' || /^\[5,\s*"OnJsonApiEvent"\s*\]$/.test(message.toString())) { clients.add(client); start(); }
      });
      client.on('close', () => clients.delete(client));
    });
  });
  await new Promise((ok, fail) => { server.once('error', fail); server.listen(0, '127.0.0.1', ok); });
  return { port: server.address().port, password, actions,
    report: () => ({ expected: writes.length, consumed: nextWrite, mismatches }),
    close() {
      if (!closePromise) {
        closed = true;
        clearTimeout(timer);
        closePromise = (async () => {
          for (const client of ws.clients) client.terminate();
          await new Promise(ok => ws.close(ok));
          server.closeAllConnections(); await new Promise(ok => server.close(ok));
        })();
      }
      return closePromise;
    },
  };
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (!process.argv[2]) throw new Error('Usage: node scripts/lcu-replay.mjs recording.jsonl [config.json] [speed]');
  const replay = await createReplay(parseRecording(await readFile(process.argv[2], 'utf8')), { speed: Number(process.argv[4] || 1) });
  const config = resolve(process.argv[3] || 'artifacts/lcu-replay.json');
  try {
    await mkdir(dirname(config), { recursive: true });
    await writeFile(config, JSON.stringify({ port: replay.port, password: replay.password }));
  } catch {
    await replay.close();
    throw new Error('Cannot save replay connection config');
  }
  console.log(`Replay ready; debug LM: RUSTLM_TEST_LCU=${config}`);
  let closing = false;
  const input = createInterface({input:process.stdin});
  const close = async () => {
    if (closing) return;
    closing = true;
    input.close();
    process.stdin.pause();
    const report = replay.report();
    const passed = replayPassed(report);
    console.log(JSON.stringify({ ...report, passed }));
    const temporary = `${config}.report-${randomUUID()}.tmp`;
    let saved = false;
    try {
      await writeFile(temporary, JSON.stringify({format:'rustlm-replay-report',version:1,...report,passed})+'\n');
      await rename(temporary,`${config}.report.json`);
      saved = true;
    } catch {
      console.error('Cannot save replay report');
      await rm(temporary,{force:true}).catch(()=>{});
    }
    try { await replay.close(); }
    finally { process.exitCode = passed && saved ? 0 : 1; }
  };
  input.on('line', line => { if (line.trim() === 'stop') void close(); });
  process.once('SIGINT', close); process.once('SIGTERM', close);
}
