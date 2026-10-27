import {readFileSync} from "node:fs";
import {test} from "node:test";
import {strict as assert} from "node:assert";
import ts from "typescript";
const compile=s=>"data:text/javascript;base64,"+Buffer.from(ts.transpileModule(s,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText).toString("base64");
const policy=compile(readFileSync(new URL("../src/lib/repair-policy.ts",import.meta.url),"utf8"));
const source=readFileSync(new URL("../src/lib/network-summary.ts",import.meta.url),"utf8").replace('"./repair-policy"',JSON.stringify(policy));
const {networkSummary}=await import(compile(source));
const probe=(overrides={})=>({domain:"auth.riotgames.com",ip:"1.1.1.1",port:443,request_path:"/",tcp:true,tls:true,classification:"http_success",http:200,error:null,...overrides});
const report=probes=>({context:{region:"RU"},probes,dns:[]});
test("unavailable IPv6 and a failed alternate IP do not mark a working service broken",()=>{
 const result=networkSummary(report([probe(),probe({ip:"::1",tcp:false,tls:false,error:"os error 10051"}),probe({ip:"2.2.2.2",tcp:false,tls:false})]));
 assert.equal(result.responding,1);assert.equal(result.needsCheck.length,0);assert.equal(result.ipv6Unavailable,1);
});
test("CDN routing aliases and other regions do not become Riot service failures",()=>{
 const result=networkSummary(report([probe({domain:"opaque.pacloudflare.com",tcp:false,tls:false}),probe({domain:"feapp.euw1.lol.pvp.net",port:2099,tcp:false,tls:false})]));
 assert.equal(result.infrastructure,1);assert.equal(result.needsCheck.length,0);assert.equal(result.responding,0);
});
test("Cloudflare replies are access restrictions, not failed TLS",()=>{
 const result=networkSummary(report([probe({http:403,classification:"cloudflare_block"})]));assert.equal(result.restricted,1);assert.equal(result.needsCheck.length,0);
});
test("DNS-only names remain unconfirmed and resolver disagreement is distinct",()=>{
 const r=report([]);r.dns=[{domain:"old.riotgames.com",resolver:"system",addresses:[]},{domain:"old.riotgames.com",resolver:"google",addresses:[]},{domain:"auth.riotgames.com",resolver:"system",addresses:[]},{domain:"auth.riotgames.com",resolver:"google",addresses:["1.1.1.1"]}];
 const result=networkSummary(r);assert.deepEqual(result.unresolved,["old.riotgames.com"]);assert.deepEqual(result.brokenSystemDns,["auth.riotgames.com"]);
});
