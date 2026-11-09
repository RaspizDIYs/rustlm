import test from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
test('service helper scopes changes and restores configuration on failed start, cancellation and conflicts', {skip:process.platform!=='win32'},()=>{
 const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',fileURLToPath(new URL('./repair-service.test.ps1',import.meta.url))],{encoding:'utf8',timeout:30000,windowsHide:true});
 assert.equal(result.status,0,result.stderr);
 const report=JSON.parse(result.stdout.trim().replace(/^\uFEFF/,''));
 assert.equal(report.restart_template_parsed,true);assert.equal(report.service_scenarios,8);assert.equal(report.foreign_services_mutated,0);assert.equal(report.real_service_commands,0);
});
