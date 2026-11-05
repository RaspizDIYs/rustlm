import { test } from "node:test";
import assert from "node:assert/strict";
import {
  createChampionSearchIndex,
  searchChampions,
  switchChampionKeyboardLayout as layout,
} from "./champion-search.ts";

const champions = {
  Ари: "Ahri", "Нуну и Виллумп": "Nunu", "К'Санте": "KSante",
  "Ле Блан": "Leblanc", "Аурелион Сол": "AurelionSol", "Чо'Гат": "Chogath",
  "Рек'Сай": "RekSai", "Бел'Вет": "Belveth", "Вел'Коз": "Velkoz",
};
const aliases = {
  Nunu: ["Nunu & Willump"], KSante: ["K'Sante"], Leblanc: ["LeBlanc"],
  AurelionSol: ["Aurelion Sol"],
};
const index = createChampionSearchIndex(champions, aliases);
const names = query => searchChampions(index, query).map(([name]) => name);

test("original Russian/English and both wrong keyboard layouts find Ahri", () => {
  for (const query of ["ари", "Ари", "Ahri", "aHrI", "fhb", "фркш"])
    assert.deepEqual(names(query), ["Ари"], query);
  assert.equal(layout("fhb", "ru"), "ари");
  assert.equal(layout("фркш", "en"), "ahri");
});

test("full names, asset keys, punctuation and partial names stay searchable", () => {
  for (const [query, expected] of [
    ["Nunu & Willump", "Нуну и Виллумп"], ["willump", "Нуну и Виллумп"],
    ["K’SANTE", "К'Санте"], ["ksante", "К'Санте"],
    ["LEBLANC", "Ле Блан"], ["Aurelion Sol", "Аурелион Сол"],
    ["aurelionsol", "Аурелион Сол"], ["сол", "Аурелион Сол"],
  ]) assert.deepEqual(names(query), [expected], query);
  for (const name of Object.keys(champions)) {
    assert.deepEqual(names(name), [name]);
    assert.deepEqual(names(layout(name, "en")), [name], name);
    assert.deepEqual(names(layout(name.toLowerCase(), "en")), [name], name);
  }
  for (const [key, values] of Object.entries(aliases)) for (const name of values) {
    assert.ok(searchChampions(index, layout(name, "ru")).some(([, asset]) => asset === key), name);
  }
});

test("keyboard mapping round trips every letter and differing punctuation key", () => {
  const en = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?@#$^&|";
  const ru = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,\"№;:?/";
  assert.equal(layout(en, "ru"), ru);
  assert.equal(layout(ru, "en"), en);
  assert.equal(layout(layout(en, "ru"), "en"), en);
  assert.equal(layout(layout(ru, "en"), "ru"), ru);
  const punctuation = createChampionSearchIndex({ "Жук": "FutureBug", "Эхо": "FutureEcho" });
  assert.deepEqual(searchChampions(punctuation, ";er"), [["Жук", "FutureBug"]]);
  assert.deepEqual(searchChampions(punctuation, '"'), [["Эхо", "FutureEcho"]]);
  assert.deepEqual(searchChampions(punctuation, "'"), [["Эхо", "FutureEcho"]]);
});

test("empty, whitespace, unknown and punctuation-only queries are predictable", () => {
  assert.equal(names("").length, Object.keys(champions).length);
  assert.deepEqual(names("  \t"), names(""));
  assert.deepEqual(names("несуществующий"), []);
  assert.deepEqual(names("!!!"), []);
  assert.deepEqual(names("a".repeat(10000)), []);
});

test("new data and aliases are searchable without a built-in champion list", () => {
  const injected = createChampionSearchIndex({ "Новый Ёж": "FutureAsset" }, {
    FutureAsset: ["Future & Champion"],
  });
  for (const query of ["новый еж", "НОВЫЙ ЁЖ", "FutureAsset", "future champion", "champion",
    layout("Новый Ёж", "en"), layout("Future & Champion", "ru")])
    assert.deepEqual(searchChampions(injected, query), [["Новый Ёж", "FutureAsset"]], query);
  assert.deepEqual(searchChampions(index, "FutureAsset"), []);
});
