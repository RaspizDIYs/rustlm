import {readFileSync} from "node:fs";
import {test} from "node:test";
import {strict as assert} from "node:assert";
import ts from "typescript";
const source=ts.transpileModule(readFileSync(new URL("../src/lib/unglitch.ts",import.meta.url),"utf8"),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {confirmationRequired,actionAvailable,UNGLITCH_ACTIONS}=await import("data:text/javascript;base64,"+Buffer.from(source).toString("base64"));
const live={backend_running:true,ui_running:true,api_available:true,phase:"Lobby",force_supported:true};
test("no action starts without a live League backend",()=>{
 for(const {id} of UNGLITCH_ACTIONS){assert.equal(actionAvailable(id,null),false);assert.equal(actionAvailable(id,{...live,backend_running:false}),false);}
});
test("hidden interface can be restored even if gameflow is unavailable",()=>{
 const hidden={...live,ui_running:false,api_available:false};
 assert.equal(actionAvailable("restore",hidden),true);
 for(const id of ["reload","force_reload","hide"])assert.equal(actionAvailable(id,hidden),false);
 assert.equal(confirmationRequired("restore","ChampSelect"),false);
});
test("renderer termination is not offered; legacy action requires the supported LCU restart",()=>{
 assert.equal(UNGLITCH_ACTIONS.some(action=>action.id==="force_reload"),false);
 assert.equal(actionAvailable("force_reload",{...live,api_available:false}),false);
 assert.equal(actionAvailable("force_reload",{...live,force_supported:false}),true);
 assert.equal(actionAvailable("hide",{...live,api_available:false}),false);
});
test("confirmation covers hiding, force restart, active or unknown phases",()=>{
 assert.equal(confirmationRequired("reload","Lobby"),false);
 for(const phase of [null,"ChampSelect","ReadyCheck","InProgress"]){assert.equal(confirmationRequired("reload",phase),true);assert.equal(confirmationRequired("force_reload",phase),true);assert.equal(confirmationRequired("hide",phase),true);}
});
