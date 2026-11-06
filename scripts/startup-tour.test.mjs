import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
const source=ts.transpileModule(readFileSync(new URL('../src/lib/startup-tour.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {TOUR_STEPS,startupTourHidden}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));

test('startup tour is suppressed only by the explicit saved preference',()=>{
  assert.equal(startupTourHidden('true'),true);
  for(const value of [null,'false','','null','{}','invalid']) assert.equal(startupTourHidden(value),false);
});
test('tour covers actual menu and tool anchors with concise descriptions',()=>{
  assert.equal(new Set(TOUR_STEPS.map(step=>step.target)).size,TOUR_STEPS.length);
  const files=['../src/components/sidebar.tsx','../src/components/social-panel.tsx','../src/app/automation/page.tsx','../src/components/draft-coordination.tsx','../src/components/repairs-workspace.tsx'].map(path=>readFileSync(new URL(path,import.meta.url),'utf8')).join('\n');
  for(const step of TOUR_STEPS){
    assert.ok(step.text.length>20 && step.text.length<230);
    assert.ok(step.route.startsWith('/'));
    assert.ok(files.includes(`data-tour="${step.target}"`) || files.includes(`href: "${step.route}"`),step.target);
  }
});
