import { readFileSync } from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { checkVersions, workspaceVersions } from './check-release-version.mjs';
test('release requires matching application versions and exact tag',()=>{
  const versions={package:'0.5.24',tauri:'0.5.24',cargo:'0.5.24'};
  assert.equal(checkVersions(versions,'v0.5.24'),'0.5.24');
  assert.equal(checkVersions(versions),'0.5.24');
  for(const tag of ['v0.5.23','0.5.24','v0.5.24\n','v0.5.24;echo secret']) assert.throws(()=>checkVersions(versions,tag));
  assert.throws(()=>checkVersions({...versions,cargo:'0.5.23'}));
  assert.throws(()=>checkVersions({...versions,package:undefined}));
});
test('checked-in frontend desktop and Rust versions agree',()=>{
  assert.ok(checkVersions(workspaceVersions()));
});

test('release packaging uses the verified lockfile and keeps production features',()=>{
  const workflow=readFileSync(new URL('../.github/workflows/release.yml',import.meta.url),'utf8');
  const build=workflow.split('- name: Build Tauri app')[1];
  assert.ok(build,'release must contain the desktop build');
  const args=build.match(/^\s+args:\s*(.+)$/m)?.[1];
  assert.ok(args,'desktop build must declare arguments');
  assert.match(args,/--config\s+tauri\.ci-updater\.json\s+--\s+--locked(?:\s|$)/);
  assert.doesNotMatch(args,/goodluck-test|ignore-version-mismatches/);
});
