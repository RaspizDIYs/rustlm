import { readFileSync } from "node:fs";
import { test } from "node:test";
import { strict as assert } from "node:assert";
import ts from "typescript";
const source = ts.transpileModule(readFileSync(new URL("../src/lib/draft-coordination.ts", import.meta.url), "utf8"), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { coordinateDraft: plan, draftIntent, DraftWorkQueue, nextPreference, allowedDraftCandidates, sameDraftIntent } = await import("data:text/javascript;base64," + Buffer.from(source).toString("base64"));
const member = (id, candidates, wanted = candidates[0], flexible = true, draftChoice = null) => ({ id, accepted: true, online: true, draftChoice, draft: { sessionKey: "123", mode: "aram", candidates, banCandidates:[], preferences:{pick:candidates.map(championId=>({championId,state:!flexible && championId===wanted ? "must":"yield"})),ban:[]} } });
const solve = ms => plan(ms, "123", "aram");
test("duplicate wants give strict user the want and flexible user the alternative, independent of input order", () => {
  const ms = [member("a", [1,2], 1), member("b", [1], 1, false)];
  assert.deepEqual(solve(ms), { a:2, b:1 });
  assert.deepEqual(solve(ms.reverse()), { a:2, b:1 });
});
test("matching handles swaps and maximizes users instead of greedy picks", () => {
  assert.deepEqual(solve([member("a", [1,2]), member("b", [2,3]), member("c", [1])]), { c:1, a:2, b:3 });
});
test("insufficient candidates never duplicate; strict wants are not silently changed", () => {
  const result = solve([member("a", [1,2], 1, false), member("b", [1], 1, false)]);
  assert.equal(Object.keys(result).length, 1);
  assert.deepEqual(Object.values(result), [1]);
});
test("confirmed reservations survive suggestions, conflicts fail closed", () => {
  assert.deepEqual(solve([member("a", [1,2], 1, true, 2), member("b", [1,2])]), { b:1, a:2 });
  assert.deepEqual(solve([member("a", [1], 1, true, 1), member("b", [1], 1, true, 1)]), {});
});
test("offline, different session/mode, missing session and invalid IDs never become assignments", () => {
  const stale = member("b", [2]); stale.online = false;
  const other = member("c", [3]); other.draft.sessionKey = "124";
  const mode = member("d", [4]); mode.draft.mode = "arena";
  assert.deepEqual(solve([member("a", [-1,0,NaN,1.5,10001,2147483648,1]), stale, other, mode]), {});
  assert.deepEqual(solve([member("a", [1]), stale, other, mode]), {a:1});
  assert.deepEqual(plan([member("a", [1])], "", "classic"), {});
});
test("publishing rejects malformed pools and bounds work for five users and 30 candidates", () => {
  const candidates=Array.from({length:30},(_,i)=>i+1);
  const local={...member("a",candidates).draft,role:""};
  assert.equal(draftIntent(local).candidates.length,30);
  assert.equal(draftIntent({...local,sessionKey:"0"}),null);
  assert.equal(draftIntent({...local,preferences:{pick:[...local.preferences.pick,{championId:31,state:"yield"}],ban:[]}}),null);
  const ms=Array.from({length:5},(_,i)=>member(String(i),candidates));
  assert.equal(new Set(Object.values(solve(ms))).size,5);
  assert.deepEqual(solve([...ms,member("6",[6])]),{});
});

test("right click cycles all three decisions; unavailable greens fall back to yellow, never red",()=>{
  assert.equal(nextPreference("yield",false),"must");
  assert.equal(nextPreference("exclude"),"must"); assert.equal(nextPreference("must"),"yield"); assert.equal(nextPreference("yield"),"exclude");
  const m=member("a",[1,2,3]);
  m.draft.preferences.pick=[{championId:1,state:"must"},{championId:2,state:"yield"},{championId:3,state:"exclude"}];
  assert.deepEqual(allowedDraftCandidates(m.draft),[1]);
  m.draft.candidates=[2,3];assert.deepEqual(allowedDraftCandidates(m.draft),[2]);
  m.draft.preferences.pick[0].state="exclude";assert.deepEqual(allowedDraftCandidates(m.draft),[2]);
  m.draft.preferences.pick[1].state="exclude";assert.deepEqual(solve([m]),{});
});

test("ban pools respect exclusions and ARAM has no ban pool",()=>{
  const d=member("a",[1]).draft;d.banCandidates=[2,3];d.preferences.ban=[{championId:2,state:"yield"},{championId:3,state:"exclude"}];
  assert.deepEqual(allowedDraftCandidates(d,"ban"),[2]);
  for(const mode of ["aram","mayhem"]) {const result=draftIntent({...d,mode});assert.deepEqual(result.banCandidates,[]);assert.deepEqual(result.preferences.ban,[]);}
  assert.deepEqual(draftIntent({...d,mode:"arena"}).banCandidates,[2]);
});

const deferred = () => { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return {promise,resolve,reject}; };
test("user action waits for current poll, skips new polls and executes once", async () => {
  const work = new DraftWorkQueue(); const read = deferred(); const calls = [];
  const polling = work.poll(async () => { calls.push("read"); await read.promise; });
  await Promise.resolve();
  const action = work.run(async () => { calls.push("action"); });
  assert.equal(work.mutationPending,true);
  await work.poll(async () => { calls.push("unwanted read"); });
  assert.equal(await work.run(async () => { calls.push("duplicate action"); }),false);
  assert.deepEqual(calls,["read"]);
  read.resolve(); await polling; assert.equal(await action,true);
  assert.deepEqual(calls,["read","action"]); assert.equal(work.mutationPending,false);
});
test("epoch/unmount cancellation prevents queued unsent mutation and releases guard", async () => {
  const work = new DraftWorkQueue(); const read = deferred(); let sent = false;
  const polling = work.poll(() => read.promise);
  const action = work.run(async () => { sent=true; });
  work.cancel(); read.resolve(); await polling;
  assert.equal(await action,false); assert.equal(sent,false); assert.equal(work.mutationPending,false);
  assert.equal(await work.run(async isCurrent => { assert.equal(isCurrent(),true); sent=true; }),true);
  assert.equal(sent,true);
});
test("failed background read does not swallow waiting click; failures release mutation guard", async () => {
  const work = new DraftWorkQueue(); const read = deferred(); let sent = false;
  const polling = work.poll(() => read.promise).catch(() => undefined);
  const action = work.run(async () => { sent=true; throw new Error("action failed"); });
  read.reject(new Error("offline")); await polling;
  await assert.rejects(action,/action failed/); assert.equal(sent,true); assert.equal(work.mutationPending,false);
});
test("epoch cancellation during action revalidation prevents its unsent request", async () => {
  const work = new DraftWorkQueue(); const revalidate = deferred(); const started = deferred(); let sent = false;
  const action = work.run(async isCurrent => { started.resolve(); await revalidate.promise; if (isCurrent()) sent=true; });
  await started.promise; work.cancel(); revalidate.resolve(); await action;
  assert.equal(sent,false); assert.equal(work.mutationPending,false);
});


test("two green alternatives leave the shared champion to the player with no alternative", () => {
  const friend = member("a", [99,103]);
  const me = member("b", [99]);
  for (const m of [friend, me]) m.draft.preferences.pick.forEach(p => p.state = "must");
  assert.deepEqual(solve([friend, me]), { b:99, a:103 });
  assert.deepEqual(solve([me, friend]), { b:99, a:103 });
});


test("yellow fallback does not receive the green bonus in a contested choice", () => {
  const a = member("a", [2]);
  a.draft.preferences.pick = [{championId:1,state:"must"},{championId:2,state:"yield"}];
  const b = member("b", [2], 2, false);
  assert.deepEqual(solve([a,b]), {b:2});
});


test("ARAM simultaneous bench availability assigns Lux to sole-green player and Ahri to flexible green player", () => {
  const friend = member("a", []), me = member("b", []);
  for (const m of [friend,me]) m.draft.mode = "aram";
  friend.draft.preferences.pick = [{championId:99,state:"must"},{championId:103,state:"must"}];
  me.draft.preferences.pick = [{championId:99,state:"must"}];
  assert.deepEqual(plan([friend,me],"123","aram"), {});
  friend.draft.candidates = [99,103]; me.draft.candidates = [99];
  assert.deepEqual(plan([friend,me],"123","aram"), {b:99,a:103});
  assert.deepEqual(plan([me,friend],"123","aram"), {b:99,a:103});
});


test("shared green turn rotates only when the authoritative ledger changes", () => {
  const a=member("a",[99],99,false), b=member("b",[99],99,false);
  assert.deepEqual(solve([a,b]),{a:99});
  // Proposals, reservations and cancelled drafts do not mutate any client ledger.
  assert.deepEqual(solve([b,a]),{a:99});
  b.greenPriority=[{championId:99,againstId:"a"}];
  assert.deepEqual(solve([a,b]),{b:99});
  assert.deepEqual(solve([b,a]),{b:99});
  a.greenPriority=[{championId:99,againstId:"b"}]; b.greenPriority=[];
  assert.deepEqual(solve([b,a]),{a:99});
});

test("two greens beat rotation and turns persist across Riot account switches", () => {
  const a=member("a",[99,103]), b=member("b",[99],99,false);
  a.draft.preferences.pick.forEach(p=>p.state="must");
  a.greenPriority=[{championId:99,againstId:"b"}];
  assert.deepEqual(solve([a,b]),{b:99,a:103});
  a.gamePuuid="new-riot-account"; b.gamePuuid="another-riot-account";
  a.draft.candidates=[99];
  assert.deepEqual(solve([b,a]),{a:99});
});

test("fairness beats list order but never assignment count, greens, or reservations", () => {
  const a=member("a",[103,99]), b=member("b",[103,99]);
  for(const m of [a,b]) m.draft.preferences.pick.forEach(p=>p.state="must");
  b.greenPriority=[{championId:99,againstId:"a"}];
  assert.deepEqual(solve([a,b]),{b:99,a:103});
  a.draftChoice=99;
  assert.deepEqual(solve([a,b]),{a:99,b:103});
  a.draftChoice=null; a.draft.preferences.pick.forEach(p=>p.state="yield");
  a.greenPriority=[{championId:99,againstId:"b"}]; b.greenPriority=[];
  a.draft.candidates=[99]; b.draft.candidates=[99];
  assert.equal(solve([a,b]).b,99);
});

test("unrelated, duplicate and oversized fairness entries cannot amplify a turn", () => {
  const a=member("a",[99],99,false), b=member("b",[99],99,false);
  b.greenPriority=[{championId:99,againstId:"outsider"},{championId:103,againstId:"a"}];
  assert.deepEqual(solve([a,b]),{a:99});
  b.greenPriority=Array.from({length:121},()=>({championId:99,againstId:"a"}));
  assert.deepEqual(solve([a,b]),{a:99});
});

const current = (id, ids, order, active) => {
  const m = member(id, ids); m.draft.mode = "classic";
  Object.assign(m.draft, {pickOrder:order,pickActive:active,pickRole:"middle"});
  m.draft.preferences.pick.forEach(p=>p.state="must"); return m;
};
test("Classic actual earlier turn takes first green despite future peer sole Lux and fairness",()=>{
  const a=current("z",[99,103],1,true), b=current("a",[99],2,false);
  b.greenPriority=[{championId:99,againstId:"z"}]; b.draftChoice=99;
  assert.deepEqual(plan([b,a],"123","classic"),{z:99});
  a.draft.pickActive=false; a.draftChoice=99; b.draft.pickActive=true; b.draftChoice=null;
  b.draft.candidates=[103]; b.draft.preferences.pick.push({championId:103,state:"yield"});
  assert.deepEqual(plan([a,b],"123","classic"),{a:103});
});
test("Classic same active LCU group assigns unique choices independently of claim order",()=>{
  const a=current("z",[99,103],2,true), b=current("a",[99],2,true);
  assert.deepEqual(plan([a,b],"123","classic"),{a:99,z:103});
  assert.deepEqual(plan([b,a],"123","classic"),{a:99,z:103});
  a.draftChoice=103; assert.deepEqual(plan([a,b],"123","classic"),{a:99,z:103});
  a.draft.pickActive=false;b.draft.pickActive=false;
  assert.deepEqual(plan([a,b],"123","classic"),{});
});
test("Classic turn and role changes invalidate intent equality; invalid order cannot publish",()=>{
  const d=current("a",[99],1,true).draft;
  assert.equal(draftIntent({...d,pickOrder:100}),null);
  assert.equal(draftIntent({...d,pickOrder:-1}),null);
  assert.equal(draftIntent({...d,pickOrder:1.5}),null);
  assert.equal(draftIntent(d).pickOrder,1);
  for(const change of [{pickOrder:2},{pickActive:false},{pickRole:"top"}]) assert.equal(sameDraftIntent(d,{...d,...change}),false);
});

test("Classic fresh full LCU context discards peer old role/order Lux reservation",()=>{
  const a=current("z",[99,103],1,true), b=current("a",[99],1,true);
  a.gamePuuid="z".repeat(78);b.gamePuuid="a".repeat(78);b.draftChoice=99;
  a.draft.pickContext=[{puuid:a.gamePuuid,summonerId:"",pickOrder:1,pickActive:true,pickRole:"middle"},
    {puuid:b.gamePuuid,summonerId:"",pickOrder:2,pickActive:false,pickRole:"top"}];
  assert.deepEqual(plan([b,a],"123","classic","z"),{z:99});
  // Even a role-only swap invalidates the old same-group reservation and old profile.
  a.draft.pickContext[1].pickOrder=1;a.draft.pickContext[1].pickActive=true;
  assert.deepEqual(plan([b,a],"123","classic","z"),{z:99});
  b.draft.pickRole="top";b.draftChoice=null;
  assert.deepEqual(plan([b,a],"123","classic","z"),{a:99,z:103});
  assert.equal(draftIntent({...a.draft,pickContext:Array(6).fill(a.draft.pickContext[0])}),null);
});


test('malformed classic pick context fails closed without throwing', () => {
  assert.equal(draftIntent({ sessionKey: '123', mode: 'classic', preferences: {pick: [], ban: []}, pickContext: [null] }), null);
});
