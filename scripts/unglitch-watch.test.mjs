import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
const code = ts.transpileModule(readFileSync(new URL('../src/lib/unglitch.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { watchLabel } = await import('data:text/javascript;base64,' + Buffer.from(code).toString('base64'));

test('disabled monitoring overrides a stale recovery state and unknown states remain readable', () => {
  assert.equal(watchLabel({ enabled: false, state: 'recovering', last_result: null }), 'Выключено');
  assert.equal(watchLabel({ enabled: true, state: 'future_state', last_result: null }), 'Наблюдаем');
  assert.equal(watchLabel(null), 'Проверяем');
});

test('protected game phases are distinguished from a failed recovery', () => {
  const status = state => ({ enabled: true, state, last_result: null });
  assert.equal(watchLabel(status('protected')), 'Без вмешательства');
  assert.equal(watchLabel(status('needs_attention')), 'Нужна проверка');
  assert.equal(watchLabel(status('recovered')), 'Восстановлено');
});
