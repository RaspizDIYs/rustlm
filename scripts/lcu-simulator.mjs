import http from 'node:http';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Reuse Next's installed WebSocket implementation; never ship this server with LM.
const { WebSocketServer } = createRequire(import.meta.url)('next/dist/compiled/ws');
const readyRoute = '/lol-matchmaking/v1/ready-check';
const sessionRoute = '/lol-champ-select/v1/session';

export function scenario(name) {
  if (!['idle', 'ready', 'classic', 'aram-locked', 'aram-open', 'mayhem-open'].includes(name)) {
    throw new Error('Unknown synthetic scenario');
  }
  const aram = name.startsWith('aram') || name.startsWith('mayhem');
  const phase = name === 'idle' ? 'Lobby' : name === 'ready' ? 'ReadyCheck' : 'ChampSelect';
  return {
    flow: { phase, gameData: { gameId: 100, queue: {
      id: name.startsWith('mayhem') ? 2400 : aram ? 450 : 420,
      gameMode: name.startsWith('mayhem') ? 'ARAM_MAYHEM' : aram ? 'ARAM' : 'CLASSIC', mapId: aram ? 12 : 11,
    } } },
    ready: { state: name === 'ready' ? 'InProgress' : 'Invalid', playerResponse: 'None', timer: 1.5 },
    session: { gameId: 100, localPlayerCellId: 1,
      myTeam: [{ cellId: 1, championId: 3, assignedPosition: aram ? '' : 'middle' }],
      actions: [], benchEnabled: aram, benchChampions: [{ championId: 1 }, { championId: 2 }],
      allowSubsetChampionPicks: name.endsWith('locked'),
      timer: { phase: aram && !name.endsWith('locked') ? 'FINALIZATION' : 'BAN_PICK', totalTimeInPhase: 45000,
        adjustedTimeLeftInPhase: name.endsWith('locked') ? 43000 : 40000 },
    },
    pickable: [1, 2, 3, 4],
    subset: name.endsWith('locked') ? [3, 4] : [],
  };
}

export async function createSimulator({ port = 0, password = randomUUID(), successor = null } = {}) {
  let state = scenario('idle');
  let failure = null;
  let unavailable = new Set();
  let ux = { backend_running: true, ui_running: true, backend_pid: 100,
    ui_pid: 200, renderer_count: 1, window_responsive: true, game_running: false,
    scope_verified: true, reloadSucceeds: true };
  const actions = [];
  const requestCounts = {};
  const subscriptions = new Set();
  const ws = new WebSocketServer({ noServer: true, maxPayload: 1024 * 1024 });
  const authenticated = request => request.headers.authorization ===
    `Basic ${Buffer.from(`riot:${password}`).toString('base64')}`;
  const publish = (uri, data, eventType = 'Update') => {
    const message = JSON.stringify([8, 'OnJsonApiEvent', { uri, data, eventType }], null, 2);
    for (const socket of subscriptions) if (socket.readyState === 1) socket.send(message);
  };
  const changeScenario = name => {
    state = scenario(name);
    publish('/lol-gameflow/v1/session', state.flow);
    publish(readyRoute, state.ready);
    publish(sessionRoute, state.flow.phase === 'ChampSelect' ? state.session : null,
      state.flow.phase === 'ChampSelect' ? 'Update' : 'Delete');
  };
  const loadFixture = fixture => {
    if (!fixture || typeof fixture.flow?.phase !== 'string'
      || !fixture.ready || !fixture.session || !Array.isArray(fixture.pickable)
      || !fixture.pickable.every(Number.isInteger)) throw new Error('Invalid fixture');
    state = structuredClone(fixture);
    publish('/lol-gameflow/v1/session', state.flow);
    publish(readyRoute, state.ready);
    publish(sessionRoute, state.session);
  };
  const server = http.createServer(async (request, response) => {
    const reply = (status, data, headers = {}) => { response.writeHead(status, { 'Content-Type': 'application/json', ...headers }); response.end(JSON.stringify(data)); };
    if (!authenticated(request)) return reply(401, { errorCode: 'UNAUTHORIZED' });
    try {
      let body = '';
      for await (const chunk of request) {
        body += chunk;
        if (body.length > 1024 * 1024) return reply(413, { errorCode: 'TOO_LARGE' });
      }
      const data = body ? JSON.parse(body) : {};
      const route = request.url;
      const requestKey = `${request.method} ${route}`;
      requestCounts[requestKey] = (requestCounts[requestKey] || 0) + 1;
      if (route === '/__test/unavailable' && request.method === 'POST') {
        if (!Array.isArray(data.routes) || data.routes.length > 20 || !data.routes.every(p => typeof p === 'string' && p.startsWith('/lol-') && p.length < 200)) return reply(400, {});
        unavailable = new Set(data.routes); return reply(200, {});
      }
      if (unavailable.has(route)) return reply(404, { errorCode: 'NOT_IMPLEMENTED' });
      if (route === '/__test/scenario' && request.method === 'POST') { changeScenario(data.name); return reply(200, {}); }
      if (route === '/__test/fixture' && request.method === 'POST') { loadFixture(data); return reply(200, {}); }
      if (route === '/__test/next-connection' && request.method === 'GET' && successor) return reply(200, successor);
      if (route === '/__test/state' && request.method === 'GET') return reply(200, { ...state, ux, actions, requestCounts });
      if (route === '/__test/ux-health' && request.method === 'GET') return reply(200, ux);
      if (route === '/__test/ux-health' && request.method === 'POST') {
        ux = { ...ux, ...data }; return reply(200, {});
      }
      if (route === '/__test/failure' && request.method === 'POST') {
        if (typeof data.route !== 'string' || !Number.isInteger(data.status) || data.status < 200 || data.status > 599) return reply(400, {});
        failure = data; return reply(200, {});
      }
      if (route === '/__test/disconnect' && request.method === 'POST') {
        for (const socket of subscriptions) socket.terminate();
        return reply(200, {});
      }
      let dropAppliedResponse = false;
      if (failure?.route === route && (typeof failure.completed !== 'boolean' || Boolean(data.completed) === failure.completed)) {
        const current = failure; failure = null;
        if (current.dropAfterApply && request.method === 'POST' && route.startsWith('/product-launcher/')) {
          actions.push({method:request.method,route});
          response.destroy(); return;
        }
        if ((current.dropAfterApply || current.transitionPhase) && request.method === 'POST' && route === `${readyRoute}/accept`) {
          state.ready.playerResponse = 'Accepted';
          actions.push({method:request.method,route});
          publish(readyRoute,state.ready);
          if (current.transitionPhase) {
            state.flow.phase = current.transitionPhase;
            if (Number.isSafeInteger(current.nextGameId)) state.flow.gameData.gameId = current.nextGameId;
            state.ready = {state:'Invalid',playerResponse:'None'};
          }
          if (current.dropAfterApply) response.destroy();
          else reply(current.status,{});
          return;
        }
        if (current.dropAfterApply && request.method === 'PATCH' && /^\/lol-champ-select\/v1\/session\/actions\/\d+$/.test(route)) {
          dropAppliedResponse = true;
        } else {
          if (Number.isFinite(current.delayMs)) await new Promise(ok => setTimeout(ok, Math.max(0, Math.min(1000, current.delayMs))));
          return reply(current.status, current.response ?? { message: 'synthetic private response' }, current.retryAfter === undefined ? {} : {'Retry-After':String(current.retryAfter)});
        }
      }
      if (request.method === 'GET') {
        if (route === '/lol-summoner/v1/current-summoner' && state.summoner) return reply(200, state.summoner);
        if (route === '/lol-chat/v1/me' && state.chatMe) return reply(200, state.chatMe);
        if (route === '/lol-service-status/v1/shard-data') return reply(200, { region: 'TEST' });
        if (route === '/lol-gameflow/v1/gameflow-phase') return reply(200, state.flow.phase);
        if (route === '/lol-gameflow/v1/session') return reply(200, state.flow);
        if (route === readyRoute) return reply(200, state.ready);
        if (route === sessionRoute && state.flow.phase === 'ChampSelect') return reply(200, state.session);
        if (route === '/lol-lobby-team-builder/champ-select/v1/subset-champion-list') return reply(200, state.subset || []);
        if (route === '/lol-champ-select/v1/pickable-champion-ids') return reply(200, state.pickable);
        if (route === '/lol-champ-select/v1/bannable-champion-ids') return reply(200, state.bannable || state.pickable);
      }
      if (request.method === 'POST' && route === '/riotclient/kill-and-restart-ux') {
        actions.push({ method: request.method, route });
        ux.ui_pid += 1;
        if (ux.reloadSucceeds) { ux.window_responsive = true; ux.renderer_count = 1; }
        return reply(204);
      }
      if (request.method === 'POST' && route === `${readyRoute}/accept` && state.ready.state === 'InProgress') {
        actions.push({ method: request.method, route });
        state.ready.playerResponse = 'Accepted'; publish(readyRoute, state.ready); return reply(204);
      }
      const swap = route?.match(/^\/lol-champ-select\/v1\/session\/bench\/swap\/(\d+)$/);
      if (request.method === 'POST' && swap && state.flow.phase === 'ChampSelect') {
        const id = Number(swap[1]);
        const index = state.session.benchChampions.findIndex(c => c.championId === id);
        const timer = state.session.timer;
        if (!state.session.benchEnabled || index < 0 || !state.pickable.includes(id)
          || timer.adjustedTimeLeftInPhase <= 0 || (timer.phase !== 'FINALIZATION' && !(state.session.allowSubsetChampionPicks && (state.subset || []).includes(id)))) return reply(409, {});
        state.session.benchChampions[index].championId = state.session.myTeam[0].championId;
        state.session.myTeam[0].championId = id;
        actions.push({ method: request.method, route }); publish(sessionRoute, state.session); return reply(204);
      }
      const actionMatch = route?.match(/^\/lol-champ-select\/v1\/session\/actions\/(\d+)$/);
      if (request.method === 'PATCH' && actionMatch && state.flow.phase === 'ChampSelect') {
        const action = state.session.actions.flat().find(a => a.id === Number(actionMatch[1]));
        const pool = action?.type === 'ban' ? state.bannable || state.pickable : state.pickable;
        if (!action || action.actorCellId !== state.session.localPlayerCellId || !action.isInProgress || action.completed || !pool.includes(data.championId)) return reply(409, {});
        action.championId = data.championId;
        if (data.completed === true) { action.completed = true; action.isInProgress = false; }
        actions.push({ method: request.method, route, championId: data.championId, completed: action.completed });
        publish(sessionRoute, state.session);
        if (dropAppliedResponse) { response.destroy(); return; }
        return reply(204);
      }
      if (request.method === 'GET' && route === '/lol-game-queues/v1/queues') return reply(200,[{id:1090,gameMode:'TFT',queueAvailability:'Available'}]);
      if (request.method === 'POST' && route === '/lol-lobby/v2/lobby') {
        if (data.queueId !== 1090 || !['None','Lobby'].includes(state.flow.phase)) return reply(409,{});
        state.lobby = {gameConfig:{queueId:1090}}; actions.push({method:request.method,route}); return reply(200,state.lobby);
      }
      if (request.method === 'GET' && route === '/lol-lobby/v2/lobby') return reply(state.lobby?200:404,state.lobby);
      // Unknown writes fail closed rather than pretending that all Riot commands work.
      return reply(404, { errorCode: 'NOT_IMPLEMENTED' });
    } catch { reply(400, { errorCode: 'INVALID_TEST_REQUEST' }); }
  });
  server.on('upgrade', (request, socket, head) => {
    if (!authenticated(request)) { socket.end('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n'); return; }
    ws.handleUpgrade(request, socket, head, client => {
      client.on('error', () => {});
      client.on('message', data => {
        try {
          const message = JSON.parse(data.toString());
          if (message[0] === 5 && message[1] === 'OnJsonApiEvent') subscriptions.add(client);
        } catch { client.close(1007); }
      });
      client.on('close', () => subscriptions.delete(client));
    });
  });
  await new Promise((ok, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', ok); });
  return {
    port: server.address().port, password, changeScenario, loadFixture,
    async close() {
      for (const client of ws.clients) client.terminate();
      await new Promise(ok => ws.close(ok));
      server.closeAllConnections();
      await new Promise(ok => server.close(ok));
    },
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const simulator = await createSimulator();
  if (process.argv[3]) simulator.loadFixture(JSON.parse(await readFile(process.argv[3], 'utf8')));
  const config = resolve(process.argv[2] || 'artifacts/lcu-simulator.json');
  await mkdir(dirname(config), { recursive: true });
  await writeFile(config, JSON.stringify({ port: simulator.port, password: simulator.password }));
  console.log(`Synthetic LCU: http://127.0.0.1:${simulator.port}; config: ${config}`);
  process.once('SIGINT', async () => { await simulator.close(); process.exit(0); });
  process.once('SIGTERM', async () => { await simulator.close(); process.exit(0); });
}
