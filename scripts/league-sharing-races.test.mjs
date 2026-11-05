import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

// Run the actual hook's async control flow with minimal hooks; no copied queue implementation.
const code=ts.transpileModule(readFileSync(new URL('../src/components/league-sharing.tsx',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX,target:ts.ScriptTarget.ES2022}}).outputText;
function harness(invoke){
 const states=[],effects=[];
 const react={useState(initial){const index=states.length;states.push(initial);return [initial,value=>{states[index]=typeof value==='function'?value(states[index]):value;}];},useRef:value=>({current:value}),useCallback:fn=>fn,useEffect:fn=>effects.push(fn)};
 const module={exports:{}};
 const context=vm.createContext({module,exports:module.exports,require:name=>name==='react'?react:name==='@tauri-apps/api/core'?{invoke}:name==='@/lib/tauri'?{loadSetting:async()=>false}:{},window:{addEventListener(){},removeEventListener(){}},document:{hidden:false},setInterval:()=>1,clearInterval(){}});
 vm.runInContext(code,context);
 return {hook:module.exports.useLeagueSharing(),states,effects};
}
const status={connected:true,matches_current:true,puuid:'a'.repeat(78),riot_id:'One#RU',owner_riot_id:'One#RU'};
test('failed refresh is reported as failure and does not poison later operations',async()=>{
 let fail=true;
 const {hook,states}=harness(async command=>{if(command==='league_status')return status;if(fail)throw Error('offline');return {friends:[],offers:[]};});
 assert.equal(await hook.run(async()=>{}),false);
 assert.match(states[2],/offline/);
 assert.equal(states[4],false);
 fail=false;
 assert.equal(await hook.run(async()=>{}),true);
 assert.equal(states[2],'');
});
test('visibility updates are serialized and latest opt-out wins',async()=>{
 const writes=[];let active=0,max=0;
 const {hook,states}=harness(async(command,args)=>{
  if(command==='league_status')return status;
  active++;max=Math.max(max,active);writes.push(args.visible);
  await new Promise(resolve=>setTimeout(resolve,5));active--;
  return {friends:[],offers:[]};
 });
 await Promise.all([hook.reload(true),hook.reload(false)]);
 assert.deepEqual(writes,[true,false]);assert.equal(max,1);assert.equal(states[3],false);
});
test('two clicks before a rerender execute only one mutation and one refresh',async()=>{
 let mutations=0,refreshes=0,release;
 const waiting=new Promise(resolve=>{release=resolve;});
 const {hook}=harness(async command=>{if(command==='league_status')return status;refreshes++;return {friends:[],offers:[]};});
 const first=hook.run(async()=>{mutations++;await waiting;});
 assert.equal(await hook.run(async()=>{mutations++;}),false);
 release();assert.equal(await first,true);assert.equal(mutations,1);assert.equal(refreshes,1);
});
test('explicit refresh inside an operation is not repeated',async()=>{
 let refreshes=0;
 const {hook}=harness(async command=>{if(command==='league_status')return status;refreshes++;return {friends:[],offers:[]};});
 assert.equal(await hook.run(()=>hook.reload(false)),true);assert.equal(refreshes,1);
});
test('malformed server lists become a readable failure instead of crashing the accounts page',async()=>{
 for(const broken of [null,{}, {friends:null,offers:[]}, {friends:[],offers:[{}]}, {friends:[{id:'bad',display_name:42}],offers:[]}]) {
  const {hook,states}=harness(async command=>command==='league_status'?status:broken);
  assert.equal(await hook.run(async()=>{}),false);
  assert.equal(states[1].offers.length,0);
  assert.match(states[2],/несовместимый список/);
 }
});
