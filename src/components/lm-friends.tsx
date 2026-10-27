"use client";
import {useCallback,useEffect,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {Button} from "@/components/ui/button";
import type {AccountRecord,GoodLuckFriend} from "@/lib/tauri";
export type LmFriend=GoodLuckFriend & {accounts:string[];updated_at:string};
export function LmFriends({accounts,onShare}:{accounts:AccountRecord[];onShare:(account:AccountRecord,friend:GoodLuckFriend)=>void}){
 const [gl,setGl]=useState<GoodLuckFriend[]>([]),[lm,setLm]=useState<LmFriend[]>([]),[scope,setScope]=useState("all"),[target,setTarget]=useState(""),[enabled,setEnabled]=useState(false),[busy,setBusy]=useState(false),[errors,setErrors]=useState<string[]>([]);
 const reload=useCallback(async()=>{
  const {goodluckGetFriends,loadSetting}=await import("@/lib/tauri");
  const results=await Promise.allSettled([goodluckGetFriends(),invoke<LmFriend[]>("lm_friends"),loadSetting("LmFriendsVisible",false)]);
  if(results[0].status==="fulfilled")setGl(results[0].value);
  if(results[1].status==="fulfilled")setLm(results[1].value);
  if(results[2].status==="fulfilled")setEnabled(Boolean(results[2].value));
  setErrors(results.flatMap(r=>r.status==="rejected"?[String(r.reason)]:[]));
 },[]);
 useEffect(()=>{void reload();},[reload]);
 const update=async()=>{setBusy(true);try{await invoke("lm_publish_friends");await reload();}catch(e){setErrors([String(e)]);}finally{setBusy(false);}};
 const selected=accounts.find(a=>a.Username===target);
 const render=(list:GoodLuckFriend[])=>list.length?list.map(friend=><div key={friend.id} className="flex items-center justify-between gap-2 rounded border p-2"><div><p>{friend.display_name}</p>{"updated_at" in friend&&<p className="text-muted-foreground">Обновлено {new Date(String(friend.updated_at)).toLocaleString("ru-RU")}</p>}</div><Button size="xs" disabled={!selected||!selected.EncryptedPassword} onClick={()=>selected&&onShare(selected,friend)}>Передать аккаунт</Button></div>):<p className="text-muted-foreground">Друзей в этой категории пока нет.</p>;
 return <section className="space-y-3 text-xs"><div className="rounded border border-primary/25 bg-primary/5 p-3 space-y-2"><p>Друзья Riot появятся, если оба включили обнаружение LM и подтвердили свои Riot-аккаунты; дружба GoodLuck не требуется.</p><label className="flex items-center gap-2"><input type="checkbox" checked={enabled} disabled={busy} onChange={async e=>{const next=e.target.checked;setBusy(true);try{await invoke("lm_set_visibility",{enabled:next});setEnabled(next);await reload();}catch(error){setErrors([String(error)]);}finally{setBusy(false);}}}/>Разрешить моим Riot-друзьям находить меня в LM</label><p className="text-muted-foreground">Обновляем друзей только вошедшего в League аккаунта; для общего списка войдите по очереди в каждый. Списки хранятся на сервере до 30 дней и обновляются, пока LM и League открыты. Пароли для поиска друзей не отправляются.</p><Button size="xs" disabled={busy||!enabled} onClick={()=>void update()}>Обновить друзей текущего Riot-аккаунта</Button><Button size="xs" variant="outline" disabled={busy} onClick={()=>void reload()}>Обновить список</Button></div>
 <div className="flex flex-wrap gap-2"><label>Друзья аккаунта <select aria-label="Друзья аккаунта" className="rounded border bg-background p-1" value={scope} onChange={e=>setScope(e.target.value)}><option value="all">Все аккаунты</option>{accounts.filter(a=>a.Puuid).map(a=><option key={a.Username} value={a.Puuid}>{a.RiotId||a.Username}</option>)}</select></label><label>Передавать <select aria-label="Аккаунт для передачи" className="rounded border bg-background p-1" value={target} onChange={e=>setTarget(e.target.value)}><option value="">Выберите аккаунт</option>{accounts.filter(a=>a.EncryptedPassword).map(a=><option key={a.Username} value={a.Username}>{a.RiotId||a.Username}</option>)}</select></label></div>
 {errors.map((error,i)=><p role="alert" className="break-words text-amber-500" key={i}>{error}</p>)}
 <div className="grid gap-3 sm:grid-cols-2"><section className="space-y-2"><h2 className="font-semibold">Друзья Riot в LM</h2>{render(lm.filter(f=>scope==="all"||f.accounts.includes(scope)))}</section><section className="space-y-2"><h2 className="font-semibold">Друзья GoodLuck</h2>{render(gl)}</section></div></section>;
}
