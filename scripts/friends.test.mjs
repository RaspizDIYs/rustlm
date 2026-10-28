import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import ts from 'typescript';
const source=ts.transpileModule(readFileSync(new URL('../src/lib/friends.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {friendsError}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
test('discovery gives separate actions for unverified ownership, closed League and disconnected GoodLuck',()=>{
 assert.match(friendsError('Подтвердите владение этим Riot-аккаунтом'),/Подтвердите владение/);
 assert.match(friendsError('LCU lockfile unavailable'),/Откройте League/);
 assert.match(friendsError('401 Unauthorized'),/Войдите в GoodLuck/);
 assert.equal(friendsError('unknown private diagnostic'),'Не удалось обновить друзей. Проверьте подключение и повторите попытку.');
});
