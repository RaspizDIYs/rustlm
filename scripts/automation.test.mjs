import { readFileSync } from "node:fs";
import { test } from "node:test";
import { strict as assert } from "node:assert";
import ts from "typescript";
const source = ts.transpileModule(readFileSync(new URL("../src/lib/automation.ts", import.meta.url), "utf8"), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { editorProfile, allowedSpells, banPriorities, setBanPriority, setPickPriority } = await import("data:text/javascript;base64," + Buffer.from(source).toString("base64"));
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


test("three ban priorities preserve old ban and swap duplicates without mutating the profile", () => {
 const legacy = {Ban:"Lux",Picks:[]};
 assert.deepEqual(banPriorities(legacy),["Lux","",""]);
 const next=setBanPriority(legacy,1,"Ahri");
 assert.deepEqual(next,{Ban:"Lux",Bans:["Lux","Ahri",""],Picks:[]});
 assert.deepEqual(setBanPriority({...legacy,...next},0,"Ahri"),{Ban:"Ahri",Bans:["Ahri","Lux",""],Picks:[]});
 assert.deepEqual(setBanPriority({...legacy,...next},0,""),{Ban:"",Bans:["","Ahri",""],Picks:[]});
 assert.deepEqual(legacy,{Ban:"Lux",Picks:[]});
});


test("assigning a pick clears every matching ban and assigning a ban clears matching picks", () => {
 const original={Picks:["Ashe","Lux","Ahri"],Ban:"Garen",Bans:["Garen","Lux",""]};
 const picked=setPickPriority(original,0,"Lux");
 assert.deepEqual(picked,{Picks:["Lux","Ashe","Ahri"],Bans:["Garen","",""],Ban:"Garen"});
 const banned=setBanPriority(original,0,"Ahri");
 assert.deepEqual(banned,{Picks:["Ashe","Lux",""],Bans:["Ahri","",""],Ban:"Ahri"});
 assert.deepEqual(original.Picks,["Ashe","Lux","Ahri"]);
});


test("legacy overlap is hidden from bans while preserving the chosen pick", () => {
 assert.deepEqual(banPriorities({Picks:["Lux"],Ban:"Lux"}),["","",""]);
});
