import { readFileSync } from "node:fs";
import { test } from "node:test";
import { strict as assert } from "node:assert";
import ts from "typescript";
const source = ts.transpileModule(readFileSync(new URL("../src/lib/automation.ts", import.meta.url), "utf8"), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { editorProfile, allowedSpells } = await import("data:text/javascript;base64," + Buffer.from(source).toString("base64"));
test("existing picks migrate only to the shared classic profile without changing saved data", () => {
  const settings = { PickChampion1: "Alistar", PickChampion2: "Nautilus", Spell1Id: 4, Spell2Id: 14, SelectedRunePageName: "Tank" };
  const before = structuredClone(settings);
  assert.deepEqual(editorProfile(settings, "classic").Picks, ["Alistar", "Nautilus", ""]);
  assert.equal(editorProfile(settings, "classic").RunePage, "Tank");
  for (const key of ["classic:jungle", "classic:middle", "aram", "mayhem", "arena"]) {
    const p = editorProfile(settings, key);
    assert.deepEqual(p.Picks, ["", "", ""]);
    assert.equal(p.AutoPick, false);
    assert.equal(p.AutoBan, false);
  }
  assert.deepEqual(settings, before);
});
test("editing one role cannot replace another role or its priority list", () => {
  const settings = { Profiles: {"classic:utility": {Picks:["Alistar"], AutoPick:true}, "classic:jungle":{Picks:["Vi"], AutoPick:true}} };
  assert.deepEqual(editorProfile(settings, "classic:utility").Picks, ["Alistar"]);
  assert.deepEqual(editorProfile(settings, "classic:jungle").Picks, ["Vi"]);
});
test("spell selectors match mode restrictions and keep Exhaust in ordinary ARAM", () => {
  assert.ok(allowedSpells("aram").includes(3));
  assert.ok(!allowedSpells("mayhem").includes(3));
  for (const mode of ["aram", "mayhem"]) {
    assert.ok(allowedSpells(mode).includes(32));
    assert.ok(allowedSpells(mode).includes(13));
    assert.ok(!allowedSpells(mode).includes(11));
    assert.ok(!allowedSpells(mode).includes(12));
  }
  assert.deepEqual(allowedSpells("arena"), []);
  assert.ok(!allowedSpells("classic").includes(32));
});
