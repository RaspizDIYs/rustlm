import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";

const require = createRequire(import.meta.url);
const ts = require("typescript");
const code = ts.transpileModule(readFileSync(new URL("./party-invitations.tsx", import.meta.url), "utf8"), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2020 },
}).outputText;

const gateModule = {};
vm.runInNewContext(ts.transpileModule(readFileSync(new URL("./party-dialog-request.ts", import.meta.url), "utf8"), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: gateModule });
const account = (Server, Username, EncryptedPassword = "encrypted") => ({ Server, Username, EncryptedPassword, RiotId: "Player#EUW" });
const snapshot = () => ({ me: "me", active: null, queues: [{ id: 450, name: "ARAM" }], invitations: [{ id: "party", revision: 3, leaderId: "leader", queueId: 450, members: [{ id: "leader", name: "Friend", server: "EUW", lobbyCount: 2, lobbyReady: true }] }] });
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function harness(api = {}) {
  const calls = [];
  let props = { snapshot: snapshot(), accounts: [account("EUW1", "ok"), account("RU", "wrong"), account("EUW", "locked", "")], onSnapshot: value => { props.snapshot = value; } };
  const slots = [], cleanups = [], effects = [], timers = [], notices = [];
  let cursor = 0, unmounted = false, writesAfterUnmount = 0;
  const react = {
    useState(initial) {
      const i = cursor++;
      if (!(i in slots)) slots[i] = initial;
      return [slots[i], value => {
        if (unmounted) writesAfterUnmount++;
        slots[i] = typeof value === "function" ? value(slots[i]) : value;
      }];
    },
    useRef(initial) { const i = cursor++; return slots[i] ??= { current: initial }; },
    useCallback(fn) { const i = cursor++; return slots[i] ??= fn; },
    useEffect(fn) { const i = cursor++; if (!(i in slots)) { slots[i] = true; effects.push(fn); } },
  };
  const exports = {};
  vm.runInNewContext(code, {
    exports,
    require(name) {
      if (name === "lucide-react") return { Users: "Users" };
      if (name === "react") return react;
      if (name === "react/jsx-runtime") return { jsx: (type, props) => ({ type, props }), jsxs: (type, props) => ({ type, props }) };
      if (name === "sonner") return { toast: { success: text => notices.push(text), error: text => notices.push(text) } };
      if (name === "@/lib/party") return { allowedSizes: id => id === 420 ? [1,2] : [1,2,3,4,5], partySnapshot: async () => props.snapshot, partyJoinWithAccount: async (...args) => { calls.push(args); }, partyAction: async () => {}, ...api };
      if (name === "@/lib/lol-assets") return { profileIconUrl: id => id ? `icon/${id}` : null };
      if (name === "./party-dialog-request") return gateModule;
      if (name === "@/components/game-mode-icon") return { GameModeIcon: "GameModeIcon" };
      if (name.startsWith("@/components/ui/")) return new Proxy({}, { get: (_, name) => name });
      throw new Error(`Unexpected import: ${name}`);
    },
    setInterval(fn) { timers.push(fn); return timers.length; },
    clearInterval() {},
  });
  function render() {
    cursor = 0;
    const tree = exports.PartyInvitations(props);
    for (const effect of effects.splice(0)) cleanups.push(effect());
    return tree;
  }
  function nodes(tree, type) {
    if (!tree || typeof tree !== "object") return [];
    if (Array.isArray(tree)) return tree.flatMap(node => nodes(node, type));
    return [...(tree.type === type ? [tree] : []), ...nodes(tree.props?.children, type)];
  }
  return {
    render, nodes, calls, update(value) { props = { ...props, ...value }; },
    button(label) { return nodes(render(), "Button").find(node => node.props.children === label); },
    unmount() { unmounted = true; for (const cleanup of cleanups) cleanup?.(); },
    get writesAfterUnmount() { return writesAfterUnmount; },
  };
}


test("filters activated accounts by normalized server; renders authoritative count and grouped select", () => {
 const h = harness(); const tree = h.render();
 assert.deepEqual(h.nodes(tree, "SelectItem").map(item => item.props.value), ["ok"]);
 assert.equal(h.nodes(tree, "SelectContent")[0].props.children.type, "SelectGroup");
 assert.equal(h.nodes(tree, "span").find(node => node.props["aria-label"] === "Участники лобби").props.children.join(""), "2/5");
 assert.equal(h.button("Перезайти и присоединиться").props.disabled, true);
 h.unmount();
});
test("selection survives snapshots, duplicates are guarded, and disappearing invitation remains pending", async () => {
 const pending = deferred(); let count = 0;
 const h = harness({ partyJoinWithAccount: () => { count++; return pending.promise; } }); h.render();
 h.nodes(h.render(), "Select")[0].props.onValueChange("ok");
 h.update({ snapshot: snapshot() });
 const click = h.button("Перезайти и присоединиться").props.onClick; click(); click(); await tick();
 assert.equal(count, 1);
 h.update({ snapshot: { ...snapshot(), invitations: [], active: snapshot().invitations[0] } });
 assert.equal(h.nodes(h.render(), "Select").length, 1);
 pending.reject(new Error("timeout")); await tick();
 assert.equal(h.button("Перезайти и присоединиться").props.disabled, false);
 assert.equal(h.nodes(h.render(), "p").some(node => node.props.role === "alert"), true);
 h.unmount();
});
test("leader readiness gates start and completion after unmount writes nothing", async () => {
 const pending = deferred(); const h = harness({ partyJoinWithAccount: () => pending.promise }); h.render();
 h.nodes(h.render(), "Select")[0].props.onValueChange("ok");
 const state = snapshot(); state.invitations[0].members[0].lobbyReady = false;
 h.update({ snapshot: state }); assert.equal(h.button("Перезайти и присоединиться").props.disabled, true);
 h.update({ snapshot: snapshot() }); h.button("Перезайти и присоединиться").props.onClick(); await tick();
 h.unmount(); pending.resolve(); await tick(); assert.equal(h.writesAfterUnmount, 0);
});


test("invites remain visible under another current Riot account and never offer its wrong server", () => {
 const h=harness();const s=snapshot();s.local={riotId:'GL Shpinat#GULAG',server:'RU'};
 h.update({snapshot:s,accounts:[account('RU','mira'),account('EUW','west')]});
 assert.deepEqual(h.nodes(h.render(),'SelectItem').map(n=>n.props.value),['west']);h.unmount();
});

test("joining is possible from both waiting and already assembled parties", () => {
 const h=harness();const s=snapshot();s.active={id:'own',leaderId:'me',state:'forming',members:[{id:'me',accepted:true},{id:'peer',accepted:false}]};
 h.update({snapshot:s});h.nodes(h.render(),'Select')[0].props.onValueChange('ok');
 assert.equal(h.button('Перезайти и присоединиться').props.disabled,false);
 s.active.members[1].accepted=true;h.update({snapshot:s});
 assert.equal(h.button('Перезайти и присоединиться').props.disabled,false);h.unmount();
});
