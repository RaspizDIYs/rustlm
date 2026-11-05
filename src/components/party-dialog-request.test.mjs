import { test } from "node:test";
import assert from "node:assert/strict";
import { createPartyRequestGate, partyActionError, partyConnected } from "./party-dialog-request.ts";

test("live Summoner ID and Riot ID establish connectivity without a PUUID", () => {
  const local = { summonerId: "123", riotId: "Player#EUW", puuid: "", error: null };
  assert.equal(partyConnected(local), true);
  assert.equal(partyConnected({ ...local, puuid: "lcu-guid" }), true);
  assert.equal(partyConnected({ ...local, summonerId: "" }), false);
  assert.equal(partyConnected({ ...local, riotId: "" }), false);
  assert.equal(partyConnected({ ...local, error: "unknown state" }), false);
  assert.equal(partyConnected(undefined), false);
});

test("HTTP failures take precedence over backend League wording", () => {
  for (const [status, expected] of [[401, /учётную запись LM/], [503, /временно недоступен/], [403, /нет прав/], [409, /Группа изменилась/]]) {
    assert.match(partyActionError(new Error(`HTTP ${status}: League party conflict`)), expected);
    assert.match(partyActionError({ status, message: "League party conflict" }), expected);
  }
  assert.match(partyActionError("League party failed"), /Не удалось выполнить действие/);
  assert.match(partyActionError("League not connected"), /Откройте League/);
  assert.match(partyActionError("Network connection failed"), /связаться с сервисом/);
});

test("a pending action blocks duplicates across close/reopen and cannot publish stale results", async () => {
  const gate = createPartyRequestGate();
  let finish;
  const pending = new Promise(resolve => { finish = resolve; });
  let published = false;
  const first = gate.run(async current => { await pending; published = current(); });
  gate.invalidate();
  assert.equal(await gate.run(async () => { throw new Error("duplicate action"); }), false);
  finish();
  await first;
  assert.equal(published, false);
  assert.equal(await gate.run(async current => { assert.equal(current(), true); }), true);
});

test("failure releases the lock and remains a failure", async () => {
  const gate = createPartyRequestGate();
  await assert.rejects(gate.run(async () => { throw new Error("lost response"); }), /lost response/);
  assert.equal(await gate.run(async () => {}), true);
});

test("a click during a poll executes once after it and takes priority over further polls", async () => {
  const gate = createPartyRequestGate();
  let finish;
  const pending = new Promise(resolve => { finish = resolve; });
  const events = [];
  const poll = gate.read(async () => { events.push("poll"); await pending; events.push("poll done"); });
  const action = gate.run(async () => { events.push("action"); });
  assert.deepEqual(events, ["poll"]);
  assert.equal(await gate.run(async () => { events.push("duplicate"); }), false);
  assert.equal(await gate.read(async () => { events.push("extra poll"); }), false);
  finish();
  await Promise.all([poll, action]);
  assert.deepEqual(events, ["poll", "poll done", "action"]);
  assert.equal(await gate.read(async () => {}), true);
});

test("closing before a queued click starts cancels it without sending an external action", async () => {
  const gate = createPartyRequestGate();
  let finish;
  const pending = new Promise(resolve => { finish = resolve; });
  const poll = gate.read(async () => { await pending; });
  let sent = 0;
  const action = gate.run(async () => { sent++; });
  gate.invalidate();
  finish();
  await poll;
  assert.equal(await action, false);
  assert.equal(sent, 0);
  assert.equal(await gate.run(async () => { sent++; }), true);
  assert.equal(sent, 1);
});
