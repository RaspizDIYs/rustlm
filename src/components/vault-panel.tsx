"use client";
import {useEffect,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {Button} from "@/components/ui/button";
import {Input} from "@/components/ui/input";
import {Card,CardHeader,CardTitle,CardContent} from "@/components/ui/card";
export function VaultPanel({authorize}:{authorize:(run:()=>Promise<void>)=>Promise<void>}){
 const [connected,setConnected]=useState(false),[code,setCode]=useState(""),[recovery,setRecovery]=useState(""),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const refresh=async()=>setConnected(await invoke<boolean>("vault_connected"));
 useEffect(()=>{void refresh().catch(()=>{});},[]);
 const act=async(action:()=>Promise<void>)=>{try{await authorize(async()=>{setBusy(true);setMessage("");try{await action();await refresh();}catch(e){setMessage(String(e));}finally{setBusy(false);}});}catch(e){setMessage(String(e));}};
 return <Card size="sm"><CardHeader><CardTitle>Мои ПК · зашифрованные аккаунты</CardTitle></CardHeader><CardContent className="space-y-2 text-xs">
 <p>На первом ПК сохраняете код заранее, затем вводите его на втором — первый ПК может быть выключен. На обоих ПК войдите в один GoodLuck; создайте хранилище на первом и введите его код на втором. Сервер получает зашифрованные аккаунты, ключ защищён Windows на каждом ПК.</p>
 <p className="text-muted-foreground">Код даёт полный доступ к хранилищу: сохраните его отдельно и передавайте только на свои ПК. Без подключённого ПК или кода восстановить данные нельзя. Общие аккаунты друзьям передаются отдельно и имеют прежние ограничения доступа.</p>
 <p className="text-muted-foreground">При переходе со старого облака добавим отсутствующие здесь аккаунты; при совпадении логина сохраним пароль и данные этого ПК.</p>
 {connected?<><p className="text-green-500">Этот ПК подключён · синхронизация каждые 30 секунд и после изменений</p><div className="flex flex-wrap gap-2">
 <Button size="xs" disabled={busy} onClick={()=>void act(async()=>{await invoke("vault_resolve",{mode:"merge"});setMessage("Аккаунты синхронизированы");})}>Синхронизировать</Button>
 <Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>{if(!window.confirm("Заменить облачные аккаунты списком с этого ПК? Изменения другого ПК могут быть потеряны."))return;await invoke("vault_resolve",{mode:"local"});setMessage("Сохранена версия этого ПК");})}>Сохранить этот ПК</Button>
 <Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>{if(!window.confirm("Заменить список аккаунтов этого ПК облачной версией?"))return;await invoke("vault_resolve",{mode:"remote"});setMessage("Загружена облачная версия");})}>Взять из облака</Button>
 <Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>{setRecovery(await invoke<string>("vault_recovery_code"));})}>Код для другого ПК</Button>
 <Button size="xs" variant="outline" disabled={busy} onClick={()=>void act(async()=>{if(!window.confirm("Отключить синхронизацию этого ПК? Аккаунты останутся здесь; затем можно ввести новый код."))return;await invoke("vault_disconnect");setRecovery("");})}>Переподключить этот ПК</Button>
 <Button size="xs" variant="destructive" disabled={busy} onClick={()=>void act(async()=>{if(!window.confirm("Сменить ключ? Остальные ПК потеряют синхронизацию до ввода нового кода; их локальные копии останутся."))return;setRecovery(await invoke<string>("vault_rotate"));})}>Отключить остальные ПК</Button>
 </div></>:<><div className="flex flex-wrap gap-2"><Button size="xs" disabled={busy} onClick={()=>void act(async()=>{setRecovery(await invoke<string>("vault_connect",{code:null}));})}>Создать на первом ПК</Button>
 <Input aria-label="Код подключения хранилища" type="password" autoComplete="off" className="h-7 max-w-sm" value={code} onChange={e=>setCode(e.target.value)} placeholder="Код с первого ПК"/>
 <Button size="xs" disabled={busy||!code.trim()} onClick={()=>void act(async()=>{await invoke("vault_connect",{code});setCode("");await invoke("vault_resolve",{mode:"merge"});setMessage("ПК подключён");})}>Подключить этот ПК</Button></div></>}
 {recovery&&<div className="rounded border border-amber-500/40 p-2"><p>Секретный код подключения — сохраните его сейчас:</p><Input aria-label="Секретный код подключения" readOnly value={recovery} onFocus={e=>e.target.select()} className="mt-1 font-mono"/><Button size="xs" variant="ghost" onClick={()=>setRecovery("")}>Скрыть код</Button></div>}
 {busy&&<p role="status">Выполняем…</p>}{message&&<p role="status" className="break-words">{message}</p>}
 </CardContent></Card>;
}
