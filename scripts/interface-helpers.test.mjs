import {readFileSync} from "node:fs";
import {test} from "node:test";
import {strict as assert} from "node:assert";
import ts from "typescript";
async function load(path){const source=ts.transpileModule(readFileSync(new URL(path,import.meta.url),"utf8"),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;return import("data:text/javascript;base64,"+Buffer.from(source).toString("base64"));}
const {rankBadge}=await load("../src/lib/rank.ts");
const {highlightParts}=await load("../src/lib/highlight.ts");
const {pickCount,jungleSpells}=await load("../src/lib/automation.ts");
test("rank chips contain only division and actual LP, without inventing missing LP",()=>{
 assert.deepEqual(rankBadge("EMERALD I 75LP"),{tier:"Emerald",label:"I 75"});
 assert.deepEqual(rankBadge("GRANDMASTER 0 LP"),{tier:"Grandmaster",label:"0"});
 assert.deepEqual(rankBadge("GOLD IV"),{tier:"Gold",label:"IV"});
 assert.deepEqual(rankBadge("Unranked"),{tier:undefined,label:""});
});
test("search highlighting treats regex characters literally and preserves original text",()=>{
 const parts=highlightParts("Ари [a+b] АРИ", "ари");assert.equal(parts.filter(p=>p.match).length,2);
 assert.equal(parts.map(p=>p.text).join(""),"Ари [a+b] АРИ");
 assert.equal(highlightParts("[a+b]", "[a+b]").filter(p=>p.match).length,1);
 assert.equal(highlightParts("hello", "").filter(p=>p.match).length,0);
});
test("ARAM counts preserve hidden priorities and jungle spells retain the selected non-Smite slot",()=>{
 const profile={Picks:Array.from({length:30},(_,i)=>String(i)), PickCount:3};
 assert.equal(pickCount(profile,"aram"),3);assert.equal(profile.Picks.length,30);
 assert.equal(pickCount({...profile,PickCount:30},"mayhem"),30);
 assert.equal(pickCount({...profile,PickCount:30},"classic"),3);
 assert.deepEqual(jungleSpells({Spell1Id:4,Spell2Id:14}),{Spell1Id:11,Spell2Id:14});
 assert.deepEqual(jungleSpells({Spell1Id:4,Spell2Id:11}),{Spell1Id:4,Spell2Id:11});
});
