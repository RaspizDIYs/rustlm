import { readFileSync } from "node:fs";
import { test } from "node:test";
import { strict as assert } from "node:assert";
import ts from "typescript";
const code=ts.transpileModule(readFileSync(new URL("../src/lib/party-roles.ts",import.meta.url),"utf8"),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {partyRoleIssue:issue,choosePartyRole:choose}=await import("data:text/javascript;base64,"+Buffer.from(code).toString("base64"));
const pair=(role,secondaryRole)=>({role,secondaryRole});
test("two distinct roles or fill are required",()=>{
 assert.ok(issue([pair('top','')]));assert.ok(issue([pair('top','top')]));
 assert.equal(issue([pair('top','jungle')]),null);assert.equal(issue([pair('fill','')]),null);
});
test("duplicate pairs are forbidden in both orders; overlapping one role is allowed",()=>{
 assert.ok(issue([pair('utility','bottom'),pair('bottom','utility')]));
 assert.equal(issue([pair('top','jungle'),pair('top','middle')]),null);
});
test("party role coverage scales with size and caps at five",()=>{
 assert.ok(issue([pair('top','jungle'),pair('top','middle'),pair('jungle','middle')]));
 assert.equal(issue([pair('top','jungle'),pair('top','middle'),pair('jungle','utility')]),null);
 assert.ok(issue([pair('top','jungle'),pair('top','middle'),pair('jungle','utility'),pair('middle','utility')]));
 const four=[pair('top','jungle'),pair('top','middle'),pair('jungle','utility'),pair('middle','bottom')];
 assert.equal(issue(four),null);assert.equal(issue([...four,pair('utility','bottom')]),null);
});
test("fill covers all positions, multiple fills are permitted",()=>{
 assert.equal(issue([pair('fill',''),pair('fill','')]),null);
 assert.equal(issue([pair('fill',''),pair('top','jungle'),pair('middle','utility')]),null);
 assert.ok(issue([pair('fill',''),pair('top','jungle'),pair('jungle','top')]));
});
test("choosing fill clears secondary; choosing secondary as primary swaps the pair",()=>{
 assert.deepEqual(choose('top','jungle','fill',1),{role:'fill',secondaryRole:''});
 assert.deepEqual(choose('top','jungle','jungle',1),{role:'jungle',secondaryRole:'top'});
});
