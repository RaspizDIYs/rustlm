"use client";
import {useEffect,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {Button} from "@/components/ui/button";
import {Input} from "@/components/ui/input";
import {Card,CardHeader,CardTitle,CardContent} from "@/components/ui/card";
import type {SyncStatus} from "@/lib/tauri";
export function VaultPanel({authorize,status,onDelete}:{authorize:(run:()=>Promise<void>)=>Promise<void>;status:SyncStatus;onDelete:()=>void}) {
 const [connected,setConnected]=useState(false),[code,setCode]=useState(""),[recovery,setRecovery]=useState(""),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const refresh=async()=>setConnected(await invoke<boolean>("vault_connected"));
 useEffect(()=>{void refresh().catch(()=>{});},[status.type]);
 const act=async(action:()=>Promise<void>)=>{try{await authorize(async()=>{setBusy(true);setMessage("");try{await action();await refresh();}catch(e){setMessage(String(e));}finally{setBusy(false);}});}catch(e){setMessage(String(e));}};
 const issue=message || (connected&&status.type==="Error"?status.message:"");
 const conflict=issue.includes("SYNC_CONFLICT");
 return <Card size="sm"><CardHeader><CardTitle>Мои ПК · облачное хранилище</CardTitle></CardHeader><CardContent className="space-y-2 text-xs">
 <p>Один общий список аккаунтов на разных ПК.</p>
 <p className={connected?"text-green-500":"text-muted-foreground"}>{connected?(status.type==="Syncing"?"Синхронизируем…":"Этот ПК подключён · изменения передаются автоматически"):"Не подключено · локальные аккаунты работают как обычно"}</p>
 {connected?<div className="flex flex-wrap gap-2">
 <Button size="xs" disabled={busy} onClick={()=>void act(async()=>{await invoke("vault_resolve",{mode:"merge"});setMessage("Синхронизация завершена");})}>Синхронизировать</Button>
 <Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>setRecovery(await invoke<string>("vault_recovery_code")))}>Подключить другой ПК</Button>
 </div>:<div className="flex flex-wrap gap-2"><Button size="xs" disabled={busy} onClick={()=>void act(async()=>setRecovery(await invoke<string>("vault_connect",{code:null})))}>Создать хранилище</Button><Input aria-label="Код подключения хранилища" type="password" autoComplete="off" className="h-7 max-w-xs" value={code} onChange={e=>setCode(e.target.value)} placeholder="Код с другого ПК"/><Button size="xs" disabled={busy||!code.trim()} onClick={()=>void act(async()=>{await invoke("vault_connect",{code});setCode("");await invoke("vault_resolve",{mode:"merge"});setMessage("Этот ПК подключён");})}>Подключиться по коду</Button></div>}
 {conflict&&<div role="alert" className="rounded border border-amber-500/40 p-2 space-y-2"><p>На двух ПК различаются данные совпавших аккаунтов. Чьи данные оставить для этих совпадений? Аккаунты, которые есть только на одном ПК, сохранятся.</p><div className="flex flex-wrap gap-2"><Button size="xs" disabled={busy} onClick={()=>void act(async()=>{await invoke("vault_resolve",{mode:"local"});setMessage("Совпадения разрешены в пользу этого ПК");})}>Данные этого ПК</Button><Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>{await invoke("vault_resolve",{mode:"remote"});setMessage("Совпадения разрешены в пользу облака");})}>Данные из облака</Button></div></div>}
 {recovery&&<div className="rounded border border-amber-500/40 p-2 space-y-1"><p>Сохраните этот секретный код в менеджере паролей и введите на втором ПК. Код даёт доступ ко всем аккаунтам хранилища.</p><Input aria-label="Секретный код подключения" readOnly value={recovery} onFocus={e=>e.target.select()} className="font-mono"/><Button size="xs" variant="ghost" onClick={()=>setRecovery("")}>Скрыть код</Button></div>}
 {busy&&<p role="status">Выполняем…</p>}{issue&&!conflict&&<p role="status" className="break-words">{issue}</p>}
 <details><summary className="cursor-pointer text-muted-foreground">Дополнительно</summary><div className="mt-2 space-y-2"><p>Списки объединяются; конфликты решаете вы. Изменения и удаления синхронизируются.</p><p>Аккаунты зашифрованы. Для восстановления нужен код или подключённый ПК; передача друзьям работает отдельно.</p>{connected&&<div className="flex flex-wrap gap-2"><Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>{if(!window.confirm("Отключить этот ПК от синхронизации? Локальные аккаунты останутся."))return;await invoke("vault_disconnect");setRecovery("");})}>Отключить этот ПК</Button><Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>{if(!window.confirm("Сменить ключ? Другим ПК понадобится новый код; их локальные копии останутся."))return;setRecovery(await invoke<string>("vault_rotate"));})}>Сменить секретный код</Button><Button size="xs" variant="destructive" disabled={busy} onClick={onDelete}>Удалить облачное хранилище</Button></div>}</div></details>
 </CardContent></Card>;
}
