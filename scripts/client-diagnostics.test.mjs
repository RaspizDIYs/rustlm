import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
const source = ts.transpileModule(readFileSync(new URL('../src/lib/unglitch.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { diagnosticArea, diagnosticOutcome, diagnosticUiLabel } = await import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));

test('HTTP response and server confirmation are not presented as proof of a repaired UI', () => {
  assert.equal(diagnosticOutcome('response'), 'Ответ получен');
  assert.equal(diagnosticOutcome('lcu_confirmed'), 'Подтверждено LCU');
  assert.equal(diagnosticOutcome('not_confirmed'), 'Не подтверждено');
});
test('missing, partial and synthetic accessibility are labelled honestly', () => {
  assert.equal(diagnosticUiLabel(undefined), 'Проверяем');
  assert.equal(diagnosticUiLabel({ state: 'limited' }), 'Мало данных');
  assert.equal(diagnosticUiLabel({ state: 'available', partial: true }), 'Частично доступно');
  assert.equal(diagnosticUiLabel({ state: 'simulated' }), 'Тестовый LCU');
  assert.equal(diagnosticUiLabel({ state: 'future' }), 'Нет данных');
  assert.equal(diagnosticArea('loot'), 'Ремесло');
});
