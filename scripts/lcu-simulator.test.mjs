import { test } from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { createSimulator } from './lcu-simulator.mjs';
const { WebSocket } = createRequire(import.meta.url)('next/dist/compiled/ws');

test('loopback simulator authenticates HTTP/WS, publishes events and enforces bench availability', async () => {
  const simulator = await createSimulator();
  const base = `http://127.0.0.1:${simulator.port}`;
  const authorization = `Basic ${Buffer.from(`riot:${simulator.password}`).toString('base64')}`;
  const request = (route, data) => fetch(base + route, { headers: { authorization },
    method: data === undefined ? 'GET' : 'POST', body: data === undefined ? undefined : JSON.stringify(data) });
  let socket;
  try {
    assert.equal((await fetch(base + '/lol-gameflow/v1/session')).status, 401);
    assert.equal((await request('/unknown')).status, 404);
    socket = new WebSocket(`ws://127.0.0.1:${simulator.port}`, { headers: { authorization } });
    await once(socket, 'open');
    socket.send('[5,"OnJsonApiEvent"]');
    // A ping/pong round trip guarantees the preceding subscription has been consumed.
    const pong = once(socket, 'pong'); socket.ping(); await pong;
    const event = once(socket, 'message');
    assert.equal((await request('/__test/scenario', { name: 'ready' })).status, 200);
    const [payload] = await event;
    assert.equal(JSON.parse(payload)[2].data.phase, 'ReadyCheck');
    assert.equal((await request('/lol-matchmaking/v1/ready-check/accept', {})).status, 204);
    assert.equal((await (await request('/lol-matchmaking/v1/ready-check')).json()).playerResponse, 'Accepted');
    await request('/__test/scenario', { name: 'aram-locked' });
    assert.equal((await request('/lol-champ-select/v1/session/bench/swap/1', {})).status, 409);
    await request('/__test/scenario', { name: 'aram-open' });
    assert.equal((await request('/lol-champ-select/v1/session/bench/swap/1', {})).status, 204);
    assert.equal((await (await request('/lol-champ-select/v1/session')).json()).myTeam[0].championId, 1);
    await request('/__test/failure', { route: '/lol-gameflow/v1/session', status: 429 });
    assert.equal((await request('/lol-gameflow/v1/session')).status, 429);
    assert.equal((await request('/lol-gameflow/v1/session')).status, 200);
    const closed = once(socket, 'close');
    await request('/__test/disconnect', {}); await closed;
  } finally {
    socket?.terminate(); await simulator.close();
  }
});

test('unknown scenarios and malformed requests do not silently change state', async () => {
  const simulator = await createSimulator();
  try {
    const base = `http://127.0.0.1:${simulator.port}`;
    const headers = { authorization: `Basic ${Buffer.from(`riot:${simulator.password}`).toString('base64')}` };
    assert.equal((await fetch(base + '/__test/scenario', { method: 'POST', headers, body: '{' })).status, 400);
    assert.equal((await fetch(base + '/__test/scenario', { method: 'POST', headers, body: '{"name":"unknown"}' })).status, 400);
    assert.equal(await (await fetch(base + '/lol-gameflow/v1/gameflow-phase', { headers })).json(), 'Lobby');
  } finally { await simulator.close(); }
});

test('an unavailable optional route stays missing without disabling ready-check', async () => {
  const simulator = await createSimulator();
  try {
    const base = `http://127.0.0.1:${simulator.port}`;
    const headers = { authorization: `Basic ${Buffer.from(`riot:${simulator.password}`).toString('base64')}` };
    const configure = routes => fetch(base + '/__test/unavailable', { method: 'POST', headers, body: JSON.stringify({ routes }) });
    assert.equal((await configure(['/lol-service-status/v1/shard-data'])).status, 200);
    for (let i = 0; i < 2; i++) assert.equal((await fetch(base + '/lol-service-status/v1/shard-data', { headers })).status, 404);
    assert.equal((await fetch(base + '/lol-matchmaking/v1/ready-check', { headers })).status, 200);
    assert.equal((await configure(['https://example.com'])).status, 400);
    assert.equal((await configure([])).status, 200);
    assert.equal((await fetch(base + '/lol-service-status/v1/shard-data', { headers })).status, 200);
  } finally { await simulator.close(); }
});

test('saved synthetic snapshots replay without mutating the fixture and invalid snapshots are rejected', async () => {
  const simulator = await createSimulator();
  try {
    const fixture = JSON.parse(await readFile(new URL('./fixtures/lcu/aram-open.json', import.meta.url), 'utf8'));
    const base = `http://127.0.0.1:${simulator.port}`;
    const headers = { authorization: `Basic ${Buffer.from(`riot:${simulator.password}`).toString('base64')}` };
    const post = body => fetch(base + '/__test/fixture', { method: 'POST', headers, body: JSON.stringify(body) });
    assert.equal((await post(fixture)).status, 200);
    fixture.session.myTeam[0].championId = 999;
    assert.equal((await (await fetch(base + '/__test/state', { headers })).json()).session.myTeam[0].championId, 3);
    assert.equal((await post({ flow: { phase: 'ChampSelect' } })).status, 400);
    assert.equal((await (await fetch(base + '/__test/state', { headers })).json()).session.myTeam[0].championId, 3);
  } finally { await simulator.close(); }
});
