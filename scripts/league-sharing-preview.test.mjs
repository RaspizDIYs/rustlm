import { test } from "node:test";
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import vm from "node:vm";

const source = readFileSync(new URL("./fixtures/league-sharing-browser.js", import.meta.url), "utf8");
function actor(side, storage) {
  const context = vm.createContext({
    URLSearchParams, location: { search: `?side=${side}` }, crypto: { randomUUID },
    localStorage: { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) },
    window: { addEventListener() {} }, document: {},
  });
  vm.runInContext(source, context);
  return context.window;
}
test("two browser actors can offer, accept and decline without any GoodLuck session", async () => {
  const storage = new Map(), owner = actor("sender", storage), friend = actor("recipient", storage);
  const a = owner.__TAURI_INTERNALS__.invoke, b = friend.__TAURI_INTERNALS__.invoke;
  assert.equal(await a("goodluck_get_user"), null);
  assert.equal(await b("goodluck_get_user"), null);
  const [account] = await a("load_accounts");
  const { friends: [recipient] } = await a("league_refresh", { visible: true });
  await a("league_share", { recipient: recipient.id.slice(7), username: account.Username, hours: 168 });
  const { offers: [offer] } = await b("league_refresh", { visible: true });
  assert.equal(offer.incoming, true);
  assert.equal(offer.metadata.owner, account.RiotId);
  assert.equal(offer.metadata.status, "pending");
  await assert.rejects(a("league_decide", { id: offer.metadata.id, action: "accept" }));
  await b("league_decide", { id: offer.metadata.id, action: "accept" });
  assert.equal((await a("league_refresh", { visible: true })).offers[0].metadata.status, "accepted");
  await assert.rejects(b("league_login", { id: offer.metadata.id }), /реальный вход отключён/);
  await b("league_decide", { id: offer.metadata.id, action: "decline" });
  assert.equal((await a("league_refresh", { visible: true })).offers.length, 0);
  assert.equal(owner.__previewCalls.includes("goodluck_get_friends"), false);
  assert.equal(friend.__previewCalls.includes("goodluck_get_friends"), false);
});
