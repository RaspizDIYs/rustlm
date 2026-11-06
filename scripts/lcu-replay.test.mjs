import test from 'node:test';
import assert from 'node:assert/strict';
import { parseRecording, createReplay, replayPassed } from './lcu-replay.mjs';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { mkdtemp, writeFile, readFile, rm, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
const { WebSocket } = createRequire(import.meta.url)('next/dist/compiled/ws');
const route = '/lol-champ-select/v1/session/actions/1';

test('actual replay CLI exits unsuccessfully on missing actions and successfully after consumption', {timeout:15000}, async () => {
  const folder = await mkdtemp(join(tmpdir(),'rustlm-replay-cli-'));
  try {
    const file = join(folder,'trace.jsonl');
    await writeFile(file, [
      {format:'rustlm-lcu-session',version:1},
      {kind:'http',at_ms:0,method:'PATCH',route,status:204,request:{championId:99},data:null},
    ].map(JSON.stringify).join('\n')+'\n');
    for (const {consume, reportFailure} of [
      {consume:false,reportFailure:false}, {consume:true,reportFailure:false}, {consume:true,reportFailure:true},
    ]) {
      const config = join(folder,`config-${consume}-${reportFailure}.json`);
      if (reportFailure) await mkdir(`${config}.report.json`);
      const child = spawn(process.execPath,[fileURLToPath(new URL('./lcu-replay.mjs',import.meta.url)),file,config],{stdio:['pipe','pipe','pipe'],windowsHide:true});
      const deadline = setTimeout(()=>child.kill(),5000);
      let output = '';
      let errors = '';
      child.stdout.on('data', chunk => {output += chunk;});
      child.stderr.on('data', chunk => {errors += chunk;});
      const exited = once(child,'close');
      try {
        while (!output.includes('Replay ready')) {
          const next = await Promise.race([once(child.stdout,'data').then(()=>true),exited.then(()=>false)]);
          assert.equal(next,true,errors);
        }
        if (consume) {
          const connection = JSON.parse(await readFile(config,'utf8'));
          const headers = {Authorization:`Basic ${Buffer.from(`riot:${connection.password}`).toString('base64')}`};
          assert.equal((await fetch(`http://127.0.0.1:${connection.port}${route}`,{method:'PATCH',headers,body:'{"championId":99}'})).status,204);
        }
        child.stdin.end('stop\n');
        const [code] = await exited;
        assert.equal(code,consume && !reportFailure?0:1,errors);
        const result = JSON.parse(output.trim().split('\n').at(-1));
        assert.equal(result.passed,consume);
        assert.equal(result.consumed,consume?1:0);
        if (reportFailure) {
          assert.match(errors,/Cannot save replay report/);
        } else {
          const saved = JSON.parse(await readFile(`${config}.report.json`,'utf8'));
          assert.equal(saved.format,'rustlm-replay-report');
          assert.equal(saved.passed,consume);
          assert.equal(saved.consumed,result.consumed);
          assert.equal(saved.password,undefined);
        }
      } finally {
        clearTimeout(deadline);
        if (child.exitCode === null) { child.kill(); await exited; }
      }
    }
  } finally {await rm(folder,{recursive:true,force:true});}
});

test('diagnostic sample is bounded but mismatch count is complete', async () => {
  const replay = await createReplay([]);
  const headers = {Authorization:`Basic ${Buffer.from(`riot:${replay.password}`).toString('base64')}`};
  try {
    for (let i = 0; i < 1005; i++) {
      assert.equal((await fetch(`http://127.0.0.1:${replay.port}${route}`,{method:'POST',headers,body:'{}'})).status,409);
    }
    assert.equal(replay.actions.length,1000);
    assert.equal(replay.report().mismatches,1005);
  } finally { await Promise.all([replay.close(), replay.close()]); }
});

test('simultaneous duplicate writes consume a recorded action only once', async () => {
  const replay = await createReplay([{kind:'http',at_ms:0,method:'PATCH',route,status:204,request:{championId:99},data:null}]);
  try {
    const headers = {Authorization:`Basic ${Buffer.from(`riot:${replay.password}`).toString('base64')}`};
    const results = await Promise.all(Array.from({length:10},()=>fetch(`http://127.0.0.1:${replay.port}${route}`,{
      method:'PATCH',headers,body:JSON.stringify({championId:99}),
    })));
    assert.equal(results.filter(r=>r.status===204).length,1);
    assert.equal(results.filter(r=>r.status===409).length,9);
    assert.deepEqual(replay.report(),{expected:1,consumed:1,mismatches:9});
  } finally { await replay.close(); }
});

test('large same-time batches preserve event order and closing cancels future events', async () => {
  const path = '/lol-gameflow/v1/gameflow-phase';
  const entries = Array.from({length:5000},(_,i)=>({kind:'Update',method:'WS',route:path,at_ms:0,data:i}));
  entries.push({kind:'Update',method:'WS',route:path,at_ms:1800000,data:'future'});
  const replay = await createReplay(entries);
  try {
    const headers = {Authorization:`Basic ${Buffer.from(`riot:${replay.password}`).toString('base64')}`};
    const deadline = performance.now()+3000;
    let value;
    do {
      value = await (await fetch(`http://127.0.0.1:${replay.port}${path}`,{headers})).json();
    } while(value!==4999 && performance.now()<deadline);
    assert.equal(value,4999);
  } finally { await Promise.all([replay.close(),replay.close()]); }
});

test('replay verdict fails on missing actions or unexpected writes', () => {
  assert.equal(replayPassed({expected:2,consumed:2,mismatches:0}),true);
  assert.equal(replayPassed({expected:2,consumed:1,mismatches:0}),false);
  assert.equal(replayPassed({expected:2,consumed:2,mismatches:1}),false);
});

test('recording rejects inconsistent transport and request shapes', () => {
  const header = JSON.stringify({format:'rustlm-lcu-session',version:1});
  const base = {kind:'http',method:'GET',route,status:200,at_ms:0};
  for (const entry of [null,[],{...base,method:'WS'},{...base,kind:'Update'},
    {...base,request:[]},{...base,request:'invalid'}]) {
    assert.throws(() => parseRecording(`${header}\n${JSON.stringify(entry)}\n`),/Invalid recording entry/);
  }
});

test('premature action does not consume the expected write', async () => {
  const replay = await createReplay([{kind:'http',at_ms:150,method:'PATCH',route,status:204,request:{championId:99},data:null}]);
  try {
    const headers = {Authorization:`Basic ${Buffer.from(`riot:${replay.password}`).toString('base64')}`};
    const url = `http://127.0.0.1:${replay.port}${route}`;
    // Start explicitly so the assertion is about replay time, not server setup.
    await fetch(url, {headers});
    const send = () => fetch(url, {method:'PATCH',headers,body:JSON.stringify({championId:99})});
    const early = await send();
    assert.equal(early.status,409);
    assert.equal((await early.json()).errorCode,'REPLAY_ACTION_TOO_EARLY');
    assert.equal(replay.report().consumed,0);
    await new Promise(resolve => setTimeout(resolve,170));
    assert.equal((await send()).status,204);
    assert.deepEqual(replay.report(),{expected:1,consumed:1,mismatches:1});
  } finally { await replay.close(); }
});
test('replay verifies real writes, rejects duplicates and never accepts unrecorded commands', async () => {
  const replay = await createReplay([{kind:'http', at_ms:0, method:'PATCH', route, status:204, request:{championId:99,completed:true},data:null}]);
  try {
    const headers = {Authorization:`Basic ${Buffer.from(`riot:${replay.password}`).toString('base64')}`};
    const url = `http://127.0.0.1:${replay.port}${route}`;
    assert.equal((await fetch(url, {method:'PATCH',headers,body:JSON.stringify({championId:1,completed:true})})).status,409);
    assert.equal((await fetch(url, {method:'PATCH',headers,body:JSON.stringify({championId:99,completed:true})})).status,204);
    assert.equal((await fetch(url, {method:'PATCH',headers,body:JSON.stringify({championId:99,completed:true})})).status,409);
    assert.deepEqual(replay.report(),{expected:1,consumed:1,mismatches:2});
    assert.equal((await fetch(url)).status,401);
  } finally { await replay.close(); }
});
test('recording parser rejects unsafe routes, versions and backwards clocks', () => {
  const header = JSON.stringify({format:'rustlm-lcu-session',version:1});
  const entry = {kind:'http',method:'GET',route:'/lol-gameflow/v1/gameflow-phase',status:200,at_ms:2,data:'Lobby'};
  assert.equal(parseRecording(`${header}\n${JSON.stringify(entry)}`).length,1);
  for (const patch of [{route:'https://riot.example'},{route:'/rso-auth/v1/session/credentials'},{at_ms:-1},{status:999}]) {
    assert.throws(()=>parseRecording(`${header}\n${JSON.stringify({...entry,...patch})}`));
  }
  assert.throws(()=>parseRecording(`${header}\n${JSON.stringify(entry)}\n${JSON.stringify({...entry,at_ms:1})}`));
  assert.equal(parseRecording(`${header}\n${JSON.stringify(entry)}\n{"kind":`).length,1);
  assert.throws(()=>parseRecording(`${header}\n{"kind":\n${JSON.stringify(entry)}\n`));
});
test('replay publishes timed events and disconnection and never exposes future snapshots', async () => {
  const ready = '/lol-matchmaking/v1/ready-check';
  const replay = await createReplay([
    {kind:'Update',method:'WS',route:ready,at_ms:100,data:{state:'InProgress',playerResponse:'None',timer:5}},
    {kind:'Disconnect',method:'WS',route:'/lol-gameflow/v1/session',at_ms:200,data:null},
  ]);
  const headers = {Authorization:`Basic ${Buffer.from(`riot:${replay.password}`).toString('base64')}`};
  let socket;
  try {
    socket = new WebSocket(`ws://127.0.0.1:${replay.port}`,{headers}); await once(socket,'open');
    const event = once(socket,'message');
    const close = once(socket,'close');
    socket.send('[5, "OnJsonApiEvent"]');
    assert.equal((await fetch(`http://127.0.0.1:${replay.port}${ready}`,{headers})).status,404);
    const [frame] = await event;
    assert.equal(JSON.parse(frame)[2].data.state,'InProgress');
    await close;
  } finally { socket?.terminate(); await replay.close(); }
});
