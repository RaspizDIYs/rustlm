import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";

const require = createRequire(import.meta.url);
const ts = require("typescript");
const code = ts.transpileModule(readFileSync(new URL("./guest-config.tsx", import.meta.url), "utf8"), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2020 },
}).outputText;
const idle = { active: false, changed: false, pending_restore: false };
const active = { active: true, changed: false, pending_restore: true };
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function harness(api = {}, onChange = async () => {}) {
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
      if (name === "react") return react;
      if (name === "react/jsx-runtime") return { jsx: (type, props) => ({ type, props }), jsxs: (type, props) => ({ type, props }) };
      if (name === "sonner") return { toast: { success: text => notices.push(text), error: text => notices.push(text) } };
      if (name === "@/lib/tauri") return { lolCfgGuestStatus: async () => idle, ...api };
      if (name.startsWith("@/components/ui/")) return new Proxy({}, { get: (_, name) => name });
      throw new Error(`Unexpected import: ${name}`);
    },
    setInterval(fn) { timers.push(fn); return timers.length; },
    clearInterval() {},
  });
  function render() {
    cursor = 0;
    const tree = exports.GuestConfig({ presets: [{ id: "p_test", name: "Mine" }], onChange });
    for (const effect of effects.splice(0)) cleanups.push(effect());
    return tree;
  }
  function nodes(tree, type) {
    if (!tree || typeof tree !== "object") return [];
    if (Array.isArray(tree)) return tree.flatMap(node => nodes(node, type));
    return [...(tree.type === type ? [tree] : []), ...nodes(tree.props?.children, type)];
  }
  return {
    render, nodes, notices, poll: () => timers[0](),
    button(label) { return nodes(render(), "Button").find(node => node.props.children === label); },
    selectPreset() { nodes(render(), "Select").find(node => nodes(node, "SelectTrigger").some(trigger => trigger.props["aria-label"] === "Гостевой пресет")).props.onValueChange("p_test"); },
    unmount() { unmounted = true; for (const cleanup of cleanups) cleanup?.(); },
    get writesAfterUnmount() { return writesAfterUnmount; },
  };
}

test("same-render double clicks start once, and parent refresh failure releases busy", async () => {
  const pending = deferred();
  let starts = 0;
  const h = harness({ lolCfgGuestStart: () => { starts++; return pending.promise; } }, async () => { throw new Error("parent refresh failed"); });
  h.render(); await tick(); h.selectPreset();
  const click = h.button("Применить временно").props.onClick;
  click(); click(); await tick();
  assert.equal(starts, 1);
  assert.equal(h.button("Применить временно").props.disabled, true);
  pending.resolve(); await tick();
  assert.equal(h.button("Применить временно").props.disabled, false);
  assert.ok(h.notices.includes("Клавиши применены временно"));
  assert.ok(h.notices.some(text => text.includes("parent refresh failed")));
  h.button("Применить временно").props.onClick(); await tick();
  assert.equal(starts, 2);
  h.unmount();
});

test("polling ignores stale rejection and never updates state after unmount", async () => {
  const requests = [deferred(), deferred(), deferred()]; let calls = 0;
  const h = harness({ lolCfgGuestStatus: () => requests[calls++].promise });
  h.render(); await tick(); h.poll(); await tick();
  requests[1].resolve({ ...active, changed: true }); await tick();
  requests[0].reject(new Error("stale failure")); await tick();
  assert.equal(h.nodes(h.render(), "p").filter(node => node.props.role === "alert").length, 0);
  assert.ok(h.button("Восстановить…"));
  h.poll(); await tick(); h.unmount();
  requests[2].resolve(idle); await tick();
  assert.equal(h.writesAfterUnmount, 0);
});

test("old successful poll cannot overwrite a newer status", async () => {
  const first = deferred(), second = deferred(); let calls = 0;
  const h = harness({ lolCfgGuestStatus: () => (++calls === 1 ? first.promise : second.promise) });
  h.render(); await tick(); h.poll(); await tick();
  second.resolve(active); await tick(); first.resolve(idle); await tick();
  assert.ok(h.button("Восстановить…"));
  h.unmount();
});

test("restore blocks duplicate clicks and modal dismissal until operation completes", async () => {
  const pending = deferred(); let restores = 0;
  const h = harness({ lolCfgGuestStatus: async () => active, lolCfgGuestRestore: () => { restores++; return pending.promise; } });
  h.render(); await tick(); h.button("Восстановить…").props.onClick();
  const dialog = h.nodes(h.render(), "Dialog")[0];
  const cancel = h.button("Отмена").props.onClick;
  const click = h.button("Заменить исходными").props.onClick;
  click(); click(); dialog.props.onOpenChange(false); cancel(); await tick();
  assert.equal(restores, 1);
  assert.equal(h.nodes(h.render(), "Dialog")[0].props.open, true);
  pending.resolve(); await tick();
  assert.equal(h.nodes(h.render(), "Dialog")[0].props.open, false);
  h.unmount();
});

test("a failed post-action status refresh still releases busy", async () => {
  let calls = 0;
  const h = harness({ lolCfgGuestStatus: async () => { if (++calls > 1) throw new Error("status failed"); return active; }, lolCfgGuestRestore: async () => {} });
  h.render(); await tick(); h.button("Восстановить…").props.onClick();
  h.button("Заменить исходными").props.onClick(); await tick();
  assert.equal(h.button("Проверить восстановление").props.disabled, false);
  h.unmount();
});

test("both select menus contain their items inside a house SelectGroup", async () => {
  const h = harness(); h.render(); await tick();
  const contents = h.nodes(h.render(), "SelectContent");
  assert.equal(contents.length, 3);
  for (const content of contents) {
    assert.equal(content.props.children.type, "SelectGroup");
    assert.ok(h.nodes(content.props.children, "SelectItem").length > 0);
  }
  h.unmount();
});


test("automatic shared preset choice needs no manual apply and saves once despite double selection", async () => {
  const pending = deferred(); let selections = 0, starts = 0;
  const h = harness({
    lolCfgGuestSetDefault: () => { selections++; return pending.promise; },
    lolCfgGuestStart: async () => { starts++; },
  });
  h.render(); await tick();
  assert.equal(starts, 0);
  const select = h.nodes(h.render(), "Select").find(node => h.nodes(node, "SelectTrigger").some(trigger => trigger.props["aria-label"] === "Личный пресет для общих аккаунтов"));
  select.props.onValueChange("p_test"); select.props.onValueChange("p_test"); await tick();
  assert.equal(selections, 1);
  assert.equal(starts, 0);
  pending.resolve(); await tick();
  assert.equal(h.button("Проверить восстановление").props.disabled, false);
  h.unmount();
});
