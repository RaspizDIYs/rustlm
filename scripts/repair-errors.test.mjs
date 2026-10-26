import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import { test } from "node:test";
import ts from "typescript";
const source = readFileSync(new URL("../src/lib/repair-errors.ts", import.meta.url), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { repairError } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
test("CLIXML and winws output never become the user-facing explanation", () => {
  const raw = '#< CLIXML <Objs><S>Preparing modules</S></Objs> RESTART_NOT_STARTED: VALIDATION_FAILED: Loaded 175 hosts';
  const message = repairError(raw);
  assert.equal(message.title, "Не удалось проверить параметры Zapret");
  assert.match(message.action, /не останавливался/);
  assert.doesNotMatch(JSON.stringify(message), /CLIXML|Loaded 175|Preparing modules/);
});
test("preflight problems have actionable messages", () => {
  for (const code of ["VPN_DETECTED", "ZAPRET_NOT_RUNNING", "STRATEGY_MISMATCH", "ZAPRET_DETAILS_UNAVAILABLE"]) {
    const message = repairError(code);
    assert.doesNotMatch(message.title, /Не удалось завершить/);
    assert.match(message.action, /стратег|VPN/);
  }
});
test("a failed rollback stays visible even when validation was the original error", () => {
  assert.match(repairError("VALIDATION_FAILED. Откат не завершён: error").action, /Откат не завершён/);
});
