"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, X, Swords, LogIn, Loader2, ShieldOff } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { WithTooltip } from "@/components/ui/with-tooltip";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import type { LmFriend } from "@/components/lm-friends";

type Status = { connected: boolean; riot_id: string | null; puuid: string | null; owner_riot_id: string | null; matches_current: boolean };
type Offer = { metadata: { id: string; owner: string; recipient: string; riot_id: string; server: string; expires_at: string; status: "pending" | "accepted" }; incoming: boolean };
type Result = { friends: LmFriend[]; offers: Offer[] };

export function useLeagueSharing() {
  const [status, setStatus] = useState<Status | null>(null);
  const [data, setData] = useState<Result>({ friends: [], offers: [] });
  const [error, setError] = useState("");
  const [visible, setVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const epoch = useRef(0);
  const reload = useCallback(async (next?: boolean) => {
    const request = ++epoch.current;
    try {
      const { loadSetting } = await import("@/lib/tauri");
      const enabled = next ?? await loadSetting("LeagueFriendsVisible", false);
      const identity = await invoke<Status>("league_status");
      const result = identity.connected ? await invoke<Result>("league_refresh", { visible: enabled }) : { friends: [], offers: [] };
      if (request !== epoch.current) return;
      setStatus(identity); setData(result); setVisible(enabled); setError("");
    } catch (failure) {
      if (request !== epoch.current) return;
      setData({ friends: [], offers: [] });
      setError(String(failure));
      if (String(failure).includes("HTTP 401")) setStatus(previous => previous ? { ...previous, connected: false } : null);
    }
  }, []);
  useEffect(() => {
    void reload();
    const refresh = () => { if (!document.hidden) void reload(); };
    window.addEventListener("focus", refresh);
    const timer = setInterval(refresh, 60000);
    return () => { ++epoch.current; clearInterval(timer); window.removeEventListener("focus", refresh); };
  }, [reload]);
  const run = async (operation: () => Promise<unknown>) => {
    setBusy(true); setError("");
    try { await operation(); await reload(); }
    catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  return { status, ...data, error, visible, busy, reload, run };
}
export type LeagueSharingState = ReturnType<typeof useLeagueSharing>;

export function LeagueConnection({ league }: { league: LeagueSharingState }) {
  const [confirmConnect, setConfirmConnect] = useState(false);
  return <>
    {!league.status?.connected && <Button size="sm" variant="outline" disabled={league.busy} onClick={() => setConfirmConnect(true)}><Swords /> Подключить League</Button>}
    <Dialog open={confirmConnect} onOpenChange={(open) => { if (!league.busy) setConfirmConnect(open); }}>
      <DialogContent className="accounts-dialog max-w-sm">
        <DialogHeader><DialogTitle>Друзья League в LM</DialogTitle><DialogDescription>Без регистрации GoodLuck: LM ненадолго сменит значок профиля для подтверждения и вернёт исходный.</DialogDescription></DialogHeader>
        <p className="text-xs text-muted-foreground">Оставьте League открытой вне поиска и матча. Принимаемые аккаунты доступны только в LM; отзыв не завершает уже открытую сессию Riot.</p>
        {league.error && <p role="alert" className="text-xs text-destructive">{league.error}</p>}
        <DialogFooter><Button size="sm" variant="outline" disabled={league.busy} onClick={() => setConfirmConnect(false)}>Отмена</Button><Button size="sm" disabled={league.busy} onClick={async () => {
          await league.run(async () => { await invoke("league_connect"); await league.reload(true); setConfirmConnect(false); });
        }}>{league.busy ? <Loader2 className="animate-spin" /> : <Check />} Подключить</Button></DialogFooter>
      </DialogContent>
    </Dialog>

  </>;
}

export function LeagueOffers({ league }: { league: LeagueSharingState }) {
  const [confirm, setConfirm] = useState<Offer | null>(null);
  if (!league.offers.length) return null;
  return <section aria-label="Передачи League" className="shrink-0 overflow-hidden rounded-xl border">
    <div className="flex items-center gap-2 border-b bg-muted/20 px-3 py-2 text-xs font-medium"><Swords className="size-3.5" /> Передачи League <Badge variant="secondary">{league.offers.length}</Badge></div>
    <div className="max-h-48 overflow-y-auto">
    {league.offers.map(offer => {
      const o = offer.metadata;
      return <div key={o.id} className="flex items-center justify-between gap-3 border-b px-3 py-2 last:border-0">
        <div className="min-w-0"><div className="flex items-center gap-2 text-xs font-medium"><span className="truncate">{o.riot_id}</span><Badge variant="outline">{o.server}</Badge>{o.status === "pending" && <span className="text-[10px] text-muted-foreground">{offer.incoming ? "Предложение" : "Ждёт принятия"}</span>}</div><p className="mt-0.5 truncate text-[11px] text-muted-foreground">{offer.incoming ? `От ${o.owner}` : `Для ${o.recipient}`} · до {new Date(o.expires_at).toLocaleDateString("ru-RU")}</p></div>
        <div className="flex shrink-0 gap-1">
          {offer.incoming && o.status === "pending" ? <>
            <WithTooltip label="Отклонить"><Button variant="outline" size="icon-sm" aria-label={`Отклонить ${o.riot_id}`} disabled={league.busy} onClick={() => void league.run(() => invoke("league_decide", { id: o.id, action: "decline" }))}><X /></Button></WithTooltip>
            <Button size="sm" disabled={league.busy} onClick={() => void league.run(() => invoke("league_decide", { id: o.id, action: "accept" }))}><Check /> Принять</Button>
          </> : <>
            {offer.incoming && <WithTooltip label="Войти"><Button size="icon-sm" disabled={league.busy} aria-label={`Войти в переданный ${o.riot_id}`} onClick={() => void league.run(() => invoke("league_login", { id: o.id }))}><LogIn /></Button></WithTooltip>}
            <WithTooltip label={offer.incoming ? "Отказаться от доступа" : "Отозвать доступ"}><Button variant="outline" size="icon-sm" disabled={league.busy} aria-label={`Отозвать ${o.riot_id}`} onClick={() => setConfirm(offer)}><ShieldOff /></Button></WithTooltip>
          </>}
        </div>
      </div>;
    })}
    </div>
    <Dialog open={Boolean(confirm)} onOpenChange={(open) => !open && setConfirm(null)}><DialogContent className="accounts-dialog max-w-sm"><DialogHeader><DialogTitle>Закрыть доступ?</DialogTitle><DialogDescription>Новые входы через LM станут недоступны. Уже открытую сессию Riot это не завершит.</DialogDescription></DialogHeader><DialogFooter><Button size="sm" variant="outline" onClick={() => setConfirm(null)}>Отмена</Button><Button size="sm" disabled={league.busy} onClick={async () => { if (!confirm) return; await league.run(() => invoke("league_decide", { id: confirm.metadata.id, action: confirm.incoming ? "decline" : "revoke" })); setConfirm(null); }}>Закрыть доступ</Button></DialogFooter></DialogContent></Dialog>
  </section>;
}
