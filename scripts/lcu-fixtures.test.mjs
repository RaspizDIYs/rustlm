import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

test('upstream fixtures retain provenance and contain no identity or authorization fields', async () => {
  const root = new URL('../src-tauri/fixtures/winer/', import.meta.url);
  const manifest = JSON.parse(await readFile(new URL('provenance.json',root),'utf8'));
  assert.match(manifest.commit,/^[a-f0-9]{40}$/);
  assert.equal(manifest.files.length,3);
  assert.match(await readFile(new URL('LICENSE',root),'utf8'),/MIT License/);
  const inspect = value => {
    if (!value || typeof value !== 'object') return;
    for (const [key, child] of Object.entries(value)) {
      assert.doesNotMatch(key,/^(puuid|summonerId|summonerName|gameName|tagLine|authorization|password|token|chatDetails|header)$/i);
      inspect(child);
    }
  };
  for (const file of manifest.files) {
    assert.doesNotMatch(file.file,/[\\/]/);
    const bytes = await readFile(new URL(file.file,root));
    assert.equal(createHash('sha256').update(bytes).digest('hex'),file.sanitized_sha256);
    const session = JSON.parse(bytes);
    assert.equal(session.timer.phase,file.phase);
    inspect(session);
  }
});
