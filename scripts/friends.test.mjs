import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import ts from 'typescript';
const source=ts.transpileModule(readFileSync(new URL('../src/lib/friends.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {friendsError,mergeFriends}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
test('discovery gives separate actions for unverified ownership, closed League and disconnected GoodLuck',()=>{
 assert.match(friendsError('Подтвердите владение этим Riot-аккаунтом'),/Подтвердите владение/);
 assert.match(friendsError('LCU lockfile unavailable'),/Откройте League/);
 assert.match(friendsError('401 Unauthorized'),/Войдите в GoodLuck/);
 assert.equal(friendsError('unknown private diagnostic'),'Не удалось обновить друзей. Проверьте подключение и повторите попытку.');
});

test('the same person appears once with both sources and their matched Riot identity',()=>{
 const gl={id:'one',username:'one',display_name:'One'};
 const riot={...gl,riot_accounts:[{puuid:'verified-id',riot_id:'Name#TAG'}]};
 const result=mergeFriends([gl],[riot]);
 assert.equal(result.length,1);
 assert.deepEqual(result[0].sources,['goodluck','riot']);
 assert.equal(result[0].riot_accounts[0].riot_id,'Name#TAG');
 assert.equal(mergeFriends([],[]).length,0);
});

