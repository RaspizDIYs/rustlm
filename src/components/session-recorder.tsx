"use client";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Circle, Square, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import { WithTooltip } from "@/components/ui/with-tooltip";
type Status = { active: boolean; path: string | null; entries: number; stopped_reason?: string | null };
export function SessionRecorder({ onError }: { onError: (message: string) => void }) {
  const [status, setStatus] = useState<Status>({active:false,path:null,entries:0});
  const [busy,setBusy] = useState(false);
  const pending = useRef(false);
  const generation = useRef(0);
  useEffect(() => {
    let active = true;
    const refresh = () => {
      const started = generation.current;
      if (!pending.current) void invoke<Status>("session_recording_status").then(s => {
        if (active && started === generation.current) setStatus(old => ({...s,path:s.path ?? old.path}));
      }).catch(() => {});
    };
    refresh(); const interval = setInterval(refresh,3000);
    return () => { active=false;clearInterval(interval); };
  },[]);
  const toggle = async () => {
    if (pending.current) return;
    pending.current=true;generation.current++;setBusy(true);
    try { setStatus(await invoke<Status>(status.active ? "session_recording_stop" : "session_recording_start")); }
    catch(e) { onError(String(e)); }
    finally { pending.current=false;setBusy(false); }
  };
  return <div className="flex flex-wrap items-center gap-2">
    <WithTooltip label="Запись выбора чемпионов и готовности без логинов и токенов; до 30 минут">
      <Button size="sm" variant={status.active?"default":"secondary"} disabled={busy} onClick={()=>void toggle()}>
        {status.active?<Square/>:<Circle/>}{status.active?"Остановить запись":"Записать сессию"}
      </Button>
    </WithTooltip>
    {status.active && <span className="text-xs text-muted-foreground">{status.entries} событий</span>}
    {status.stopped_reason && <span role="status" className="text-xs text-amber-400">{status.stopped_reason}</span>}
    {status.path && <WithTooltip label="Папка записей"><Button size="icon-sm" variant="outline" aria-label="Открыть папку записей" onClick={()=>{
      void import("@tauri-apps/plugin-shell").then(({open})=>open(status.path!.replace(/[\\/][^\\/]+$/,""))).catch(e=>onError(String(e)));
    }}><FolderOpen/></Button></WithTooltip>}
  </div>;
}
