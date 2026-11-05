"use client";

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Ban, Swords, Check, ArrowLeftRight, Info } from "lucide-react";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Dialog, DialogTrigger, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { WithTooltip } from "@/components/ui/with-tooltip";
import { championIconUrl, profileIconUrl } from "@/lib/lol-assets";
import { nextPreference, type DraftLocal } from "@/lib/draft-coordination";
import { partySnapshot, type ChampionPreference, type PartyMember, type PartySnapshot } from "@/lib/party";

type Kind = "pick" | "ban";
const reasons = {
  solo: "Нужны хотя бы два игрока в одном лобби League.",
  "not-linked": "Игроки лобби ещё не связаны в RustLM.",
  disabled: "Включите совместимую автоматизацию чемпионов.",
  empty: "Добавьте чемпионов в приоритеты.",
  waiting: "Ждём данные автоматизации друзей.",
  active: "Ждём данные общего лобби.",
};

export function coordinationStatus(local: DraftLocal, snapshot: PartySnapshot | null) {
  if (local.coordinationStatus === "solo" || local.coordinationStatus === "disabled") return local.coordinationStatus;
  return snapshot?.coordinationStatus || local.coordinationStatus || "waiting";
}

export function coordinationRows(local: DraftLocal, snapshot: PartySnapshot | null): PartyMember[] {
  if (coordinationStatus(local, snapshot) !== "active" || !snapshot?.active) return [];
  const ids = new Set(snapshot.local.lobbyMemberIds.filter(Boolean));
  if (ids.size < 2) return [];
  const rows = snapshot.active.members.filter((m: PartyMember) => m.accepted && m.online && ids.has(m.gameSummonerId) && (
    m.automation ? m.automation.mode === local.mode && (m.automation.autoPick || m.automation.autoBan)
    : !!local.sessionKey && m.draft?.sessionKey === local.sessionKey && m.draft.mode === local.mode
  ));
  return rows.length >= 2 && rows.some(m => m.id === snapshot.me) ? rows : [];
}

export function useDraftPreferences(mode: string, role: string, beforeChange: () => Promise<void>) {
  const [local, setLocal] = useState<DraftLocal | null>(null);
  const [error, setError] = useState("");
  const epoch = useRef(0);
  const queue = useRef(Promise.resolve());
  const pending = useRef(0);
  const readVersion = useRef(0);
  useEffect(() => {
    const generation = ++epoch.current;
    setLocal(null); setError("");
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      const version = readVersion.current;
      if (!pending.current) try {
        const view = await invoke<DraftLocal>("draft_local", { mode, role });
        if (generation === epoch.current && !pending.current && version === readVersion.current) { setLocal(view); setError(""); }
      } catch (e) { if (generation === epoch.current && !pending.current && version === readVersion.current) setError(String(e)); }
      if (generation === epoch.current) timer = setTimeout(poll, 3000);
    }
    void poll();
    return () => { ++epoch.current; clearTimeout(timer); };
  }, [mode, role]);
  function change(name: string, kind: Kind, state?: ChampionPreference["state"]) {
    const generation = epoch.current;
    ++pending.current; ++readVersion.current;
    queue.current = queue.current.then(async () => {
      if (generation !== epoch.current) return;
      await beforeChange();
      if (generation !== epoch.current) return;
      const view = await invoke<DraftLocal>("draft_local", { mode, role });
      if (generation !== epoch.current) return;
      let championId = Object.entries(view.championNames || {}).find(([, value]) => value === name)?.[0];
      if (!championId) {
        const info = await invoke<{ id: string } | null>("get_champion_info", { displayName: name });
        if (!info) throw new Error("Не удалось определить чемпиона.");
        championId = info.id;
      }
      if (generation !== epoch.current) return;
      const id = Number(championId);
      const preference = (view.palettePreferences?.[kind] || view.preferences[kind]).find(p => p.championId === id);
      const updated = await invoke<DraftLocal>("draft_set_preference", {
        championId: id, kind,
        state: state ?? nextPreference(preference?.state || "yield", view.markedPreferences?.[kind].includes(id) ?? false),
        mode, role, sessionKey: view.sessionKey || null,
      });
      if (generation === epoch.current) { setLocal(updated); setError(""); }
    }).catch(e => { if (generation === epoch.current) setError(String(e)); }).finally(() => { --pending.current; });
  }
  function preference(name: string, kind: Kind) {
    const entry = (local?.palettePreferences?.[kind] || local?.preferences[kind])?.find(p => local.championNames?.[p.championId] === name);
    return entry && local?.markedPreferences?.[kind].includes(entry.championId) ? entry.state : undefined;
  }
  return { preference, change, error };
}

export function DraftCoordination({ mode, role }: { mode: string; role: string }) {
  const [enabled,setEnabled]=useState<boolean|null>(null);
  const [changing,setChanging]=useState(false);
  const changingRef=useRef(false);
  const mounted=useRef(false);
  useEffect(()=>{ mounted.current=true;void invoke<boolean>("draft_enabled").then(value=>{if(mounted.current)setEnabled(value);}).catch(()=>{if(mounted.current)setEnabled(null);});return()=>{mounted.current=false;};},[]);
  async function toggle(value:boolean) {
    if(changingRef.current)return;
    changingRef.current=true;setChanging(true);
    try {await invoke("draft_set_enabled",{enabled:value});if(mounted.current){setEnabled(value);setError("");}}
    catch(e){if(mounted.current)setError(String(e));}
    finally {changingRef.current=false;if(mounted.current)setChanging(false);}
  }
  const [state, setState] = useState<{ local: DraftLocal; snapshot: PartySnapshot | null } | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    setState(null); setError("");
    async function poll() {
      try {
        const [local, snapshot] = await Promise.all([
          invoke<DraftLocal>("draft_local", { mode, role }), partySnapshot().catch(() => null),
        ]);
        if (!disposed) { setState({ local, snapshot }); setError(""); }
      } catch (e) { if (!disposed) { setState(null); setError(String(e)); } }
      if (!disposed) timer = setTimeout(poll, 3000);
    }
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [mode, role, enabled]);
  const rows = enabled !== false && state ? coordinationRows(state.local, state.snapshot) : [];
  return <section aria-label="Согласование чемпионов" className="space-y-2 rounded-lg border bg-card p-3 text-sm">
    <div className="flex items-center justify-between gap-2"><div className="flex items-center gap-1.5"><span className="font-medium">Согласование с друзьями</span>
      <Dialog>
        <WithTooltip label="Как решаются спорные пики"><DialogTrigger render={<Button size="icon-sm" variant="ghost" aria-label="Как работает согласование" />}><Info className="size-3.5" /></DialogTrigger></WithTooltip>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader><DialogTitle>Как распределяются чемпионы</DialogTitle><DialogDescription>LM распределяет доступных чемпионов между участниками общего лобби с включённым согласованием.</DialogDescription></DialogHeader>
          <div className="space-y-3 text-xs leading-relaxed">
            <p><strong className="text-emerald-500">✓ Зелёный — забираю всегда.</strong> LM сначала выбирает из доступных зелёных. Если ни один зелёный недоступен, переходит к жёлтым.</p>
            <p><strong className="text-amber-400">⇄ Жёлтый — могу поменяться.</strong> Гибкий выбор: LM может уступить чемпиона другу и назначить вам другого допустимого. Жёлтые участвуют в автовыборе, когда доступных зелёных нет.</p>
            <p><strong className="text-red-500">⊘ Красный — не беру никогда.</strong> Исключается из автопика; в автобан не добавляется.</p>
            <p><strong>Одновременный выбор в ARAM / Арене.</strong> LM сравнивает доступные варианты всей группы и назначает каждому не больше одного чемпиона. Если у вас только зелёная Люкс, а у друга зелёные Люкс и Ари, вам достанется Люкс, другу — Ари, когда обе доступны. Скорость клика и одновременное открытие скамейки не определяют приоритет.</p>
            <p><strong>Обычный драфт.</strong> Первым выбирает игрок, чей ход сейчас активен. При смене очереди или роли LM пересматривает выбор; следующий игрок берёт следующий доступный приоритет.</p>
            <p><strong>Друг банит вашу зелёную Люкс?</strong> При включённом согласовании LM не автобанит разрешённые пики участников группы, даже на этапе банов. Проверяется следующий из трёх банов. Если они не подходят, выбирается случайный безопасный бан без командных зелёных приоритетов; если безопасного варианта нет, бан остаётся ручным.</p>
            <p><strong>Спор при одновременном выборе.</strong> Если один получил её в начавшейся игре, а второй остался без желаемого зелёного, в следующей игре преимущество получает второй — пока у него снова нет зелёной альтернативы.</p>
            <p className="text-muted-foreground">Подтверждённая резервация сохраняется. LM не гарантирует обмен: доступность, таймеры, действия союзников и правила League остаются решающими. Автобан настраивается отдельно.</p>
          </div>
        </DialogContent>
      </Dialog>
    </div><Switch size="sm" aria-label="Согласование с друзьями" checked={enabled===true} disabled={enabled===null || changing} onCheckedChange={value=>void toggle(value)} /></div>
    {rows.length ? <ul className="space-y-2">{rows.map(member => {
      const automation = member.automation?.mode === state!.local.mode ? member.automation : null;
      const freshDraft = !!state!.local.sessionKey && member.draft?.sessionKey === state!.local.sessionKey && member.draft.mode === state!.local.mode ? member.draft : null;
      const pools = freshDraft?.preferences || automation?.preferences;
      const icon = profileIconUrl(member.profileIconId || 0);
      return <li key={member.id} className="flex flex-wrap items-center gap-3">
        <div className="flex w-36 min-w-0 items-center gap-2"><Avatar size="sm" className="overflow-hidden">{icon && <AvatarImage className="absolute inset-0" src={icon} alt="" />}<AvatarFallback className="absolute inset-0">{member.name.slice(0,2).toUpperCase()}</AvatarFallback></Avatar><span className="truncate" title={member.name}>{member.name}</span></div>
        {(["pick", "ban"] as const).filter(kind => kind === "pick" || !["aram", "mayhem"].includes(mode)).map(kind => <div key={kind} role="group" aria-label={kind === "pick" ? "Автопик" : "Автобан"} className="flex flex-wrap items-center gap-1">
          {kind === "pick" ? <Swords aria-hidden className="size-3 text-muted-foreground" /> : <Ban aria-hidden className="size-3 text-muted-foreground" />}
          {(automation && !(kind === "pick" ? automation.autoPick : automation.autoBan) ? [] : pools?.[kind] || []).filter(p=>p.state!=="exclude").map(p => {
            const name = state!.local.championNames?.[p.championId] || `#${p.championId}`;
            return <WithTooltip key={p.championId} label={name}><span role="img" aria-label={name} className={`relative size-7 overflow-hidden rounded border-2 ${p.state === "must" ? "border-emerald-500" : "border-amber-400"}`}><img src={championIconUrl(p.championId)!} alt="" className="size-full" />{p.state==="must" ? <Check aria-hidden className="absolute bottom-0 right-0 size-3 rounded-sm bg-background/90 text-emerald-500" /> : <ArrowLeftRight aria-hidden className="absolute bottom-0 right-0 size-3 rounded-sm bg-background/90 text-amber-400" />}</span></WithTooltip>;
          })}
        </div>)}
      </li>;
    })}</ul> : <p className="text-xs text-muted-foreground">{error ? "Согласование временно недоступно." : enabled===false ? "Согласование выключено." : state ? reasons[coordinationStatus(state.local, state.snapshot)] : "Загружаем данные лобби…"}</p>}
  </section>;
}
