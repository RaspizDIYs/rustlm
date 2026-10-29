import {readFileSync} from "node:fs";
import {test} from "node:test";
import {strict as assert} from "node:assert";
import ts from "typescript";
const source=ts.transpileModule(readFileSync(new URL("../src/lib/logs.ts",import.meta.url),"utf8"),{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {logLevel,appendLogLines}=await import("data:text/javascript;base64,"+Buffer.from(source).toString("base64"));
test("levels match file and plugin logs without confusing bracketed modules",()=>{
 assert.equal(logLevel("[2026-10-29 12:00:00.000] [INFO] test"),"INFO");
 assert.equal(logLevel("[2026-10-29][12:00:00][app_lib][WARN] test"),"WARN");
 assert.equal(logLevel("continuation"),undefined);
});
test("tail stays bounded, appends only new data and replaces after rotation",()=>{
 const old=["old"]; assert.equal(appendLogLines(old,[],false),old);
 assert.deepEqual(appendLogLines(old,["new"],false),["old","new"]);
 assert.deepEqual(appendLogLines(old,["rotated"],true),["rotated"]);
 const tail=appendLogLines([],Array.from({length:2500},(_,i)=>String(i)),true);
 assert.equal(tail.length,2000); assert.equal(tail[0],"500"); assert.equal(tail.at(-1),"2499");
});
