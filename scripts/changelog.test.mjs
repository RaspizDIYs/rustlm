import {readFileSync} from "node:fs";
import {test} from "node:test";
import {strict as assert} from "node:assert";
import ts from "typescript";
const source=ts.transpileModule(readFileSync(new URL("../src/lib/changelog.ts",import.meta.url),"utf8"),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {changelogSections}=await import("data:text/javascript;base64,"+Buffer.from(source).toString("base64"));
test("version sections preserve wrapped release notes without mixing releases",()=>{
 assert.deepEqual(changelogSections("ignored\r\n## v1.2.3\r\n- First change\r\n  continued here\r\n\r\n## 1.2.2\r\n- Older change"),[{version:"1.2.3",items:["First change continued here"]},{version:"1.2.2",items:["Older change"]}]);
 assert.deepEqual(changelogSections(""),[]);
});
test("all shipped versions have their own release notes",()=>{
 const sections=changelogSections();assert.ok(sections.length>3);assert.equal(new Set(sections.map(x=>x.version)).size,sections.length);assert.ok(sections.every(x=>x.items.length));
});
