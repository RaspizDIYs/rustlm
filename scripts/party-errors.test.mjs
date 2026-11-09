import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
const code=ts.transpileModule(readFileSync(new URL('../src/components/party-dialog-request.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {partyActionError}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
test('party read failures distinguish LM authorization, rate limit, server, network and closed League',()=>{
 for(const input of ['Подключите аккаунт League к LM','session (HTTP 401)',{status:401,message:'unauthorized'}])assert.match(partyActionError(input),/панели «Друзья»/);
 assert.match(partyActionError('Дождитесь завершения подключения League'),/ещё выполняется/);
 assert.match(partyActionError('wait (HTTP 429)'),/частое обновление/);
 assert.match(partyActionError('unavailable (HTTP 503)'),/Сервис пати временно/);
 for(const input of ['LM_NETWORK_TIMEOUT: connection','Не удалось связаться с сервером друзей League']){const message=partyActionError(input);assert.match(message,/сервисом пати/);assert.doesNotMatch(message,/Откройте League/);}
 assert.match(partyActionError('Откройте League'),/Откройте League/);
 assert.match(partyActionError('changed (HTTP 409)'),/Группа изменилась/);
});
