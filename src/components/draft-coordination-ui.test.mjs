import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

function load(file, { invoke = async () => {}, seeded = [] } = {}) {
  const states = [], effects = [], timers = [];
  const jsx = (type, props) => ({ type, props });
  const react = {
    memo: x => x, useState(initial) { const i = states.length; states.push(seeded[i] ?? initial); return [states[i], value => { states[i] = typeof value === 'function' ? value(states[i]) : value; }]; },
    useRef: value => ({ current: value }), useEffect: fn => effects.push(fn),
  };
  const module = { exports: {} };
  const source = readFileSync(new URL(file, import.meta.url), 'utf8');
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true });
  assert.equal(code.diagnostics.length, 0);
  vm.runInNewContext(code.outputText, { module, exports: module.exports, require(name) {
    if (name === 'react') return react;
    if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
    if (name === '@tauri-apps/api/core') return { invoke };
    if (name === '@/lib/draft-coordination') return { nextPreference: (state, marked) => !marked ? 'must' : ({ must: 'yield', yield: 'exclude', exclude: 'must' })[state] };
    if (name === '@/lib/lol-assets') return { profileIconUrl: id => id ? `/avatar/${id}` : null, championIconUrl: id => `/champion/${id}` };
    return new Proxy({}, { get: (_, key) => key });
  }, setTimeout: fn => { timers.push(fn); return timers.length; }, clearTimeout() {} });
  return { ...module.exports, states, effects, timers };
}
const settle = () => new Promise(resolve => setImmediate(resolve));
const local = () => ({ mode: 'classic', role: 'top', sessionKey: '', coordinationStatus: 'active', championNames: { 1: 'Энни' }, preferences: { pick: [{ championId: 1, state: 'yield' }], ban: [] }, markedPreferences: { pick: [], ban: [] } });
const member = (id, summoner) => ({ id, gameSummonerId: summoner, name: id, accepted: true, online: true, automation: { mode: 'classic', autoPick: true, autoBan: false, preferences: local().preferences } });
const snapshot = () => ({ me: 'one', coordinationStatus: 'active', local: { lobbyMemberIds: ['101', '202'] }, active: { members: [member('one', '101'), member('two', '202'), member('outsider', '303')] } });

test('read-only roster requires actual group, compatible active peers, and authoritative status', () => {
  const { coordinationRows, coordinationStatus } = load('./draft-coordination.tsx');
  assert.deepEqual(Array.from(coordinationRows(local(), snapshot()), m => m.id), ['one', 'two']);
  for (const status of ['solo', 'not-linked', 'disabled', 'empty', 'waiting']) {
    const snap = snapshot(); snap.coordinationStatus = status;
    assert.equal(coordinationRows(local(), snap).length, 0);
  }
  assert.equal(coordinationStatus({ ...local(), coordinationStatus: 'disabled' }, snapshot()), 'disabled');
  assert.equal(coordinationRows({ ...local(), coordinationStatus: 'solo' }, snapshot()).length, 0);
  for (const ids of [[], ['101'], ['101', '101'], ['101', '404']]) {
    const snap = snapshot(); snap.local.lobbyMemberIds = ids;
    assert.equal(coordinationRows(local(), snap).length, 0);
  }
  for (const mutate of [m => m.online = false, m => m.accepted = false, m => m.automation.mode = 'aram', m => m.automation.autoPick = false]) {
    const snap = snapshot(); mutate(snap.active.members[1]);
    assert.equal(coordinationRows(local(), snap).length, 0);
  }
});

test('three rapid clicks persist must -> yield -> exclude, without selecting a champion, without concurrent mutations', async () => {
  let view = local(), active = 0, max = 0;
  const writes = [], order = [];
  const h = load('./draft-coordination.tsx', { invoke: async (command, args) => {
    if (command === 'draft_local') return structuredClone(view);
    assert.equal(command, 'draft_set_preference'); active++; max = Math.max(max, active);
    await Promise.resolve(); writes.push(args.state); view.preferences.pick[0].state = args.state; view.markedPreferences.pick = [1]; active--;
    return structuredClone(view);
  } });
  const hook = h.useDraftPreferences('classic', 'top', async () => { order.push('save'); });
  const cleanup = h.effects[0]();
  for (let i = 0; i < 3; i++) hook.change('Энни', 'pick');
  await settle();
  assert.deepEqual(writes, ['must', 'yield', 'exclude']); assert.equal(max, 1);
  assert.deepEqual(order, ['save', 'save', 'save']);
  assert.equal(h.states[0].preferences.pick[0].state, 'exclude'); cleanup();
});

test('failed operation does not drop the next click; unmount cancels queued and stale writes', async () => {
  let writes = 0, fail = true, release;
  const wait = new Promise(resolve => { release = resolve; });
  const h = load('./draft-coordination.tsx', { invoke: async command => {
    if (command === 'draft_local') return local(); writes++; return local();
  } });
  const hook = h.useDraftPreferences('classic', 'top', async () => { if (fail) { fail = false; throw Error('save failed'); } });
  const cleanup = h.effects[0]();
  hook.change('Энни', 'pick'); hook.change('Энни', 'pick'); await settle();
  assert.equal(writes, 1); assert.equal(h.states[1], '');
  const blocked = load('./draft-coordination.tsx', { invoke: async () => { await wait; return local(); } });
  const other = blocked.useDraftPreferences('classic', 'top', async () => {});
  const stop = blocked.effects[0](); other.change('Энни', 'pick'); await Promise.resolve(); stop(); release(); await settle();
  assert.equal(blocked.states[0], null); cleanup();
});

test('top has only the opt-in switch and no champion editing controls or fabricated rows', () => {
  const h = load('./draft-coordination.tsx', { seeded: [true, false, { local: local(), snapshot: snapshot() }] });
  const tree = h.DraftCoordination({ mode: 'classic', role: 'top' });
  const nodes = [];
  function walk(node) { if (!node || typeof node !== 'object') return; if (Array.isArray(node)) return node.forEach(walk); nodes.push(node); walk(node.props?.children); }
  walk(tree);
  assert.equal(nodes.filter(n => n.type === 'li').length, 2);
  assert.equal(nodes.filter(n => n.type === 'Switch').length, 1);
  assert.equal(nodes.filter(n => ['button', 'DropdownMenuTrigger'].includes(n.type)).length, 0);
  assert.equal(nodes.filter(n => n.props?.onClick || n.props?.onContextMenu).length, 0);
});

test('grid preserves left-click selection; right-click changes only preference and suppresses browser menu; Shift+F10 cycles preference without extra action buttons', () => {
  const h = load('./champion-grid.tsx'); const calls = [];
  const grid = h.ChampionGrid({ entries: [['Энни', 'Annie']], version: '1', selected: 'Энни', onSelect: name => calls.push(['select', name]), onPreference: (name, state) => calls.push(['preference', name, state]) });
  const item = grid.props.children[0];
  item.props.children.props.children.props.onClick();
  assert.deepEqual(calls, [['select', 'Энни']]);
  const menu = h.ChampionPreferenceMenu(item.props);
  let prevented = 0, stopped = 0;
  menu.props.onContextMenu({ preventDefault() { prevented++; }, stopPropagation() { stopped++; } });
  assert.deepEqual(calls[1], ['preference', 'Энни', undefined]); assert.equal(calls.length, 2);
  menu.props.onKeyDown({ key: 'F10', shiftKey: true, preventDefault() { prevented++; }, stopPropagation() { stopped++; } });
  assert.deepEqual(calls[2], ['preference', 'Энни', undefined]); assert.equal(prevented, 2); assert.equal(stopped, 2);
  assert.equal(menu.props.children.filter(Boolean).some(n => n.type === 'button'), false);
});


test('a slow poll cannot overwrite a completed preference mutation', async () => {
  let release, reads = 0;
  const stale = new Promise(resolve => { release = resolve; });
  const h = load('./draft-coordination.tsx', { invoke: async (command, args) => {
    if (command === 'draft_local') return ++reads === 1 ? stale : local();
    const updated = local(); updated.preferences.pick[0].state = args.state; updated.markedPreferences.pick = [1]; return updated;
  } });
  const hook = h.useDraftPreferences('classic', 'top', async () => {});
  const cleanup = h.effects[0](); hook.change('Энни', 'pick'); await settle();
  assert.equal(h.states[0].preferences.pick[0].state, 'must');
  release(local()); await settle();
  assert.equal(h.states[0].preferences.pick[0].state, 'must'); cleanup();
});


test('opt-in changes are persisted once and the switch disables while loading',async()=>{
 const calls=[];const h=load('./draft-coordination.tsx',{invoke:async(command,args)=>{calls.push([command,args]);return command==='draft_enabled'?true:null;}});
 const tree=h.DraftCoordination({mode:'classic',role:'top'});
 const sw=tree.props.children[0].props.children[1];assert.equal(sw.type,'Switch');assert.equal(sw.props.disabled,true);
 const cleanup=h.effects[0]();await settle();assert.equal(h.states[0],true);
 await sw.props.onCheckedChange(false);await settle();assert.equal(h.states[0],false);
 assert.equal(calls.filter(([c])=>c==='draft_set_enabled').length,1);cleanup();
});


test('right click marks an unselected champion without selecting a slot and first mark is green', async () => {
  const calls = [];
  const h = load('./draft-coordination.tsx', { invoke: async (command, args) => {
    calls.push([command, args]);
    if (command === 'draft_local') return local();
    if (command === 'get_champion_info') return { id: "99" };
    const updated = local();
    updated.palettePreferences = {pick:[{championId:99,state:args.state}],ban:[]};
    updated.championNames[99] = 'Люкс'; updated.markedPreferences.pick = [99];
    return updated;
  } });
  const hook = h.useDraftPreferences('classic', 'top', async () => {});
  const cleanup = h.effects[0]();
  hook.change('Люкс', 'pick'); await settle(); await settle();
  const edits = calls.filter(([command]) => command === 'draft_set_preference');
  assert.equal(edits.length, 1);
  assert.equal(edits[0][1].championId, 99);
  assert.equal(edits[0][1].state, 'must');
  assert.equal(calls.some(([command]) => command === 'save_auto_accept_settings'), false);
  assert.equal(h.states[0].palettePreferences.pick[0].state, 'must');
  assert.equal(h.states[0].preferences.pick[0].championId, 1);
  cleanup();
});


test('ban cards ignore traffic-light state and never handle preference clicks', () => {
 const h=load('./champion-grid.tsx'); let changes=0;
 const card=h.ChampionPreferenceMenu({name:'Люкс',kind:'ban',state:'must',change:()=>{changes++;},children:'champion'});
 assert.equal(card.props['data-preference'],undefined);
 assert.equal(card.props.onContextMenu,undefined);
 assert.equal(card.props.onKeyDown,undefined);
 assert.equal(card.props.children.filter(Boolean).length,1);
 assert.equal(changes,0);
});
