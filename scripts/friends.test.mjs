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

test('matching a Riot roster cannot erase an existing GoodLuck avatar',()=>{
 const gl={id:'one',username:'one',display_name:'One',avatar_url:'https://example.test/avatar.png'};
 for(const missing of [null,'',undefined,'  ']){
  const result=mergeFriends([gl],[{...gl,avatar_url:missing}]);
  assert.equal(result[0].avatar_url,gl.avatar_url);
  assert.deepEqual(result[0].sources,['goodluck','riot']);
 }
});

test('duplicate Riot entries retain their identities without inventing GoodLuck friendship',()=>{
 const person={id:'league:one',username:'One',display_name:'One'};
 const list=mergeFriends([],[{...person,riot_accounts:[{puuid:'a',riot_id:'One#RU'}]},{...person,riot_accounts:[{puuid:'b',riot_id:'Alt#EUW'}]},{...person,riot_accounts:[{puuid:'a',riot_id:'One#RU'}]}]);
 assert.equal(list.length,1);
 assert.deepEqual(list[0].sources,['riot']);
 assert.deepEqual(list[0].riot_accounts.map(a=>a.puuid),['a','b']);
 assert.equal(mergeFriends([{...person,id:'different-person'}],[person]).length,2);
});
