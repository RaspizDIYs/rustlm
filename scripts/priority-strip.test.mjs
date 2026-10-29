import {readFileSync} from "node:fs";
import {test} from "node:test";
import {strict as assert} from "node:assert";
import ts from "typescript";
const source=ts.transpileModule(readFileSync(new URL("../src/lib/automation.ts",import.meta.url),"utf8"),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {removePriority}=await import("data:text/javascript;base64,"+Buffer.from(source).toString("base64"));
test("remove a specific champion without losing priority order or inactive picks",()=>{
 const picks=["Ahri","Lux","Vi","Ashe"];
 assert.deepEqual(removePriority(picks,1,3),["Ahri","Vi","","Ashe"]);
 assert.deepEqual(picks,["Ahri","Lux","Vi","Ashe"]);
});
test("consecutive removals leave free slots for multiple new picks",()=>{
 const picks=removePriority(removePriority(["Ahri","Lux","Vi"],0,3),0,3);
 assert.deepEqual(picks,["Vi","",""]);
 assert.deepEqual(removePriority(["Ahri"],0,1),[""]);
 assert.deepEqual(removePriority(["Ahri"],-1,1),["Ahri"]);
});
