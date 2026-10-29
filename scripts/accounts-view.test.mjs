import { readFileSync } from "node:fs";
import { test } from "node:test";
import { strict as assert } from "node:assert";
import ts from "typescript";

const source = ts.transpileModule(readFileSync(new URL("../src/lib/accounts-view.ts", import.meta.url), "utf8"), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { visibleAccounts } = await import("data:text/javascript;base64," + Buffer.from(source).toString("base64"));
const account = (Username, RiotId, Server = "RU", Note = "", Pinned = false) => ({ Username, RiotId, Server, Note, Pinned, SummonerName: "" });

test("alphabetical account sorting supports Cyrillic and reverses without mutating the source", () => {
  const accounts = [account("ya", "Яна"), account("b", "Бета"), account("a", "Альфа")];
  assert.deepEqual(visibleAccounts(accounts, "", { key: "RiotId", descending: false }).map(a => a.RiotId), ["Альфа", "Бета", "Яна"]);
  assert.deepEqual(visibleAccounts(accounts, "", { key: "RiotId", descending: true }).map(a => a.RiotId), ["Яна", "Бета", "Альфа"]);
  assert.deepEqual(accounts.map(a => a.Username), ["ya", "b", "a"]);
});

test("server sorting groups services and alphabetizes accounts inside each server", () => {
  const accounts = [account("z", "Zulu", "RU"), account("b", "Beta", "EUW"), account("a", "Alpha", "RU")];
  assert.deepEqual(visibleAccounts(accounts, "", { key: "Server", descending: false }).map(a => a.Username), ["b", "a", "z"]);
});

test("login sorting ignores the GoodLuck storage prefix and compares numbers naturally", () => {
  const accounts = [account("gl:login10", "Z"), account("login2", "A"), account("gl:login1", "B")];
  assert.deepEqual(visibleAccounts(accounts, "", { key: "Username", descending: false }).map(a => a.Username), ["gl:login1", "login2", "gl:login10"]);
});

test("default order keeps pinned accounts first; explicit sorting respects the selected column", () => {
  const accounts = [account("a", "Alpha"), account("z", "Zulu", "RU", "", true)];
  assert.equal(visibleAccounts(accounts, "", null)[0].Username, "z");
  assert.equal(visibleAccounts(accounts, "", { key: "RiotId", descending: false })[0].Username, "a");
});

test("search finds notes, usernames and regions case-insensitively and can return an empty list", () => {
  const accounts = [account("home", "North", "RU", "Основной"), account("work", "Mira", "EUW")];
  assert.equal(visibleAccounts(accounts, " ОСНОВНОЙ ", null)[0].Username, "home");
  assert.equal(visibleAccounts(accounts, "WORK", null)[0].Username, "work");
  assert.equal(visibleAccounts(accounts, "euw", null)[0].Username, "work");
  assert.deepEqual(visibleAccounts(accounts, "missing", null), []);
});
