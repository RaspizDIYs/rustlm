"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Check, Crown, LogIn, LogOut, Play, Plus, RefreshCw, Square, Users, X, Shuffle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Card, CardContent } from "@/components/ui/card";
import { LolRoleIcon } from "@/components/lol-role-icon";
import { RankBadge } from "@/components/rank-badge";
import { PartyInvitations } from "@/components/party-invitations";
import { profileIconUrl } from "@/lib/lol-assets";
import { loadAccounts, type AccountRecord } from "@/lib/tauri";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { WithTooltip } from "@/components/ui/with-tooltip";
import type { GoodLuckFriend } from "@/lib/tauri";
import { allowedSizes, partyAcceptLobby, partyAction, partyCancelQueue, partyCreate, partyLobby, partyQueue, partySnapshot, type Party, type PartyRole, type PartySnapshot } from "@/lib/party";
import { partyRoleIssue, choosePartyRole } from "@/lib/party-roles";
import { cn } from "@/lib/utils";
import { GameModeIcon } from "@/components/game-mode-icon";
import { createPartyRequestGate, partyActionError, partyConnected } from "./party-dialog-request";

type LmContact = { id: `league:${string}`; name: string; avatar?: string | null };
const roles: { id: PartyRole; name: string }[] = [
  { id: "", name: "Роль" }, { id: "top", name: "Верх" }, { id: "jungle", name: "Лес" },
  { id: "middle", name: "Центр" }, { id: "bottom", name: "Низ" },
  { id: "utility", name: "Поддержка" }, { id: "fill", name: "Любая" },
];

export function PartyPanel() {
  const open = true;
  const [accounts, setAccounts] = useState<AccountRecord[]>([]);
  useEffect(() => { let disposed = false; void loadAccounts().then(value => { if (!disposed) setAccounts(value); }).catch(() => {}); return () => { disposed = true; }; }, []);
  const [snapshot, setSnapshot] = useState<PartySnapshot | null>(null);
  const [friends, setFriends] = useState<LmContact[]>([]);
  const [friendsLoaded, setFriendsLoaded] = useState(false);
  const [friendsError, setFriendsError] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [queueId, setQueueId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const gate = useRef(createPartyRequestGate());
  const friendsReadAt = useRef(0);
  const sizes = queueId === null ? null : allowedSizes(queueId, snapshot?.queues.find(queue => queue.id === queueId)?.maxPlayers);
  const maxPlayers = sizes?.[sizes.length - 1] ?? 0;
  const maxInvitees = Math.max(0, maxPlayers - 1);
  const validSize = sizes?.includes(selected.length + 1) ?? false;

  useEffect(() => {
    setSelected(ids => ids.length > maxInvitees ? ids.slice(0, maxInvitees) : ids);
  }, [maxInvitees]);

  const refresh = useCallback(async (forceFriends = false) => {
    await gate.current.read(async current => {
      try {
        const next = await partySnapshot();
        if (current()) { setSnapshot(next); setError(""); setQueueId(id => next.queues.some(q => q.id === id && allowedSizes(q.id)) ? id : next.queues.find(q => allowedSizes(q.id))?.id ?? null); }
      } catch {
        if (current()) { setError("Не удалось обновить группу — откройте League и проверьте подключение."); }
      }
      if (!current() || !forceFriends && Date.now() - friendsReadAt.current < 30000) return;
      friendsReadAt.current = Date.now();
      try {
        const contacts = await invoke<GoodLuckFriend[]>("party_friends");
        if (current()) {
          const independent = contacts.filter((f): f is GoodLuckFriend & { id: `league:${string}` } => f.id.startsWith("league:") && f.id.length > 7);
          setFriends([...new Map(independent.map(f => [f.id, { id: f.id, name: f.display_name || f.username, avatar: f.avatar_url }])).values()]);
          setSelected(ids => ids.filter(id => independent.some(f => f.id === id)));
          setFriendsLoaded(true);
          setFriendsError("");
        }
      } catch {
        if (current()) { setFriendsLoaded(false); setFriendsError("Друзья LM недоступны — откройте League и обновите список."); }
      }
    });
  }, []);

  useEffect(() => {
    gate.current.invalidate();
    if (!open) return;
    setSnapshot(null);
    setError("");
    setBusy(false);
    setFriendsLoaded(false);
    let timer: ReturnType<typeof setTimeout>;
    let disposed = false;
    const poll = async () => {
      await refresh();
      if (!disposed) timer = setTimeout(poll, 2000);
    };
    let stop: (() => void) | undefined;
    void listen<PartySnapshot>("party-changed", event => {
      if (!disposed && Array.isArray(event.payload?.invitations)) void gate.current.read(async current => {
        if (current() && !disposed) setSnapshot(event.payload);
      });
    }).then(unlisten => { if (disposed) unlisten(); else stop = unlisten; }).catch(() => {});
    void poll();
    return () => { disposed = true; clearTimeout(timer); stop?.(); gate.current.invalidate(); };
  }, [open, refresh]);

  const run = async (operation: () => Promise<unknown>) => {
    if (!open) return;
    await gate.current.run(async current => {
      let failed = false;
      try { await operation(); }
      catch (failure) { failed = true; if (current()) setError(partyActionError(failure)); }
      // A refresh reconciles conflicts and uncertain external outcomes without repeating them.
      try {
        if (current()) {
          const next = await partySnapshot();
          if (current()) setSnapshot(next);
        }
      } catch {
        if (current()) {
          setSnapshot(null);
          if (!failed) setError("Результат не подтверждён — обновите состояние группы перед следующим действием.");
        }
      } finally { if (current()) setBusy(false); }
    }, () => { setBusy(true); setError(""); });
  };

  const active = snapshot?.active;
  const connected = partyConnected(snapshot?.local);
  const leader = !!active && active.leaderId === snapshot?.me;
  const own = active?.members.find(m => m.id === snapshot?.me);
  const roleIssue = active && [400,420,440].includes(active.queueId) ? partyRoleIssue(active.members.filter(member => member.accepted)) : null;
  const validActiveSize = !!active && !!allowedSizes(active.queueId, snapshot?.queues.find(queue => queue.id === active.queueId)?.maxPlayers)?.includes(active.members.length);
  const queueName = (id: number) => snapshot?.queues.find(q => q.id === id)?.name || `Очередь ${id}`;
  const act = (party: Party, action: "leave" | "cancel") =>
    run(() => partyAction(party.id, { expectedRevision: party.revision, action }));
  const disabled = busy || !snapshot || !!error;

  return (
    <div className="flex flex-col gap-3 text-xs">
      <h1 className="flex items-center gap-2 text-lg font-semibold"><Users className="size-4" />Пати</h1>
      <Card><CardContent>
        <div className="flex items-center justify-between gap-2">
          {connected ? <div className="flex flex-wrap items-center gap-2">
            <span className="text-muted-foreground">Аккаунт:</span>
            <Avatar className="size-8"><AvatarImage src={profileIconUrl(snapshot?.local.profileIconId || 0) || undefined} alt="" /><AvatarFallback>LM</AvatarFallback></Avatar>
            <span className="font-medium">{snapshot?.local.riotId}</span>
            <Badge variant="outline">{snapshot?.local.server || "Сервер —"}</Badge>
            {snapshot?.local.rankDisplay ? <RankBadge display={snapshot.local.rankDisplay} /> : <span className="text-muted-foreground">Ранг —</span>}
          </div> : <span className="text-muted-foreground">Аккаунт: откройте League и войдите в игровой аккаунт.</span>}
          <WithTooltip label="Обновить группу">
            <Button size="sm" variant="outline" aria-label="Обновить группу" disabled={busy} onClick={() => { setError(""); void refresh(true); }}><RefreshCw /></Button>
          </WithTooltip>
        </div>
      </CardContent></Card>
      <PartyInvitations snapshot={snapshot} accounts={accounts} onSnapshot={setSnapshot} />
      <Card><CardContent className="space-y-3">
        {error && <p role="alert" className="text-destructive">{error}</p>}
        {!snapshot && !error && <p role="status" className="text-muted-foreground">Загрузка группы…</p>}



        {snapshot && !active && <section aria-label="Пригласить друзей" className="flex min-w-0 flex-col gap-3">
          <Select value={queueId === null ? null : String(queueId)} onValueChange={value => setQueueId(value === null ? null : Number(value))} disabled={busy || !connected}>
            <SelectTrigger className="w-full" aria-label="Режим игры"><SelectValue placeholder="Режим игры">{queueId === null ? null : <span className="inline-flex items-center gap-2"><GameModeIcon mode={queueId} />{queueName(queueId)}</span>}</SelectValue></SelectTrigger>
            <SelectContent><SelectGroup>{snapshot.queues.filter(queue => allowedSizes(queue.id)).map(queue => <SelectItem key={queue.id} value={String(queue.id)}><GameModeIcon mode={queue.id} />{queue.name}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
          <div className="flex items-center justify-between gap-2"><span>Друзья LM</span><span className="text-muted-foreground">{selected.length}/{maxInvitees} выбрано</span></div>
          <p className="text-muted-foreground">{maxPlayers ? `До ${maxPlayers} игроков, включая вас.` : "Нет доступных режимов игры."}</p>
          {sizes && !validSize && <p role="status" className="text-muted-foreground">Для Flex нужно 1, 2, 3 или 5 игроков.</p>}
          {friendsError && <p role="status" className="text-muted-foreground">{friendsError}</p>}
          {!friendsError && !friendsLoaded && <p role="status" className="text-muted-foreground">Загрузка друзей…</p>}
          {friendsLoaded && !friends.length && <p className="text-muted-foreground">Добавьте друзей LM в открытом клиенте League.</p>}
          <div className="flex min-w-0 max-h-60 flex-col gap-2 overflow-x-hidden overflow-y-auto px-1 py-2">
            {friends.map(friend => <label key={friend.id} className={cn("flex min-w-0 shrink-0 items-center gap-2 rounded-lg border p-2 transition-colors", selected.includes(friend.id) && "border-primary/50 bg-primary/15")}>
              <Avatar className="size-7"><AvatarImage src={friend.avatar || undefined} alt="" /><AvatarFallback>{friend.name.slice(0, 1)}</AvatarFallback></Avatar>
              <span className="min-w-0 flex-1 truncate">{friend.name}</span>
              <Switch size="sm" aria-label={`Пригласить ${friend.name}`} checked={selected.includes(friend.id)} disabled={busy || !connected || !friendsLoaded || (!selected.includes(friend.id) && selected.length >= maxInvitees)} onCheckedChange={checked => setSelected(ids => checked ? ids.includes(friend.id) || ids.length >= maxInvitees ? ids : [...ids, friend.id] : ids.filter(id => id !== friend.id))} />
            </label>)}
          </div>
          <Button size="sm" className="self-end" disabled={disabled || !connected || !friendsLoaded || !validSize || queueId === null || !snapshot.queues.some(q => q.id === queueId)} onClick={() => { if (queueId !== null && validSize) void run(() => partyCreate(queueId, selected.map(id => id.slice(7)))); }}><Plus data-icon="inline-start" />Пригласить</Button>
        </section>}

        {active && <section aria-label="Участники группы" className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-2"><span className="inline-flex items-center gap-2"><GameModeIcon mode={active.queueId} />{queueName(active.queueId)}</span><Badge variant="secondary">{active.state === "forming" ? active.members.some(m => !m.accepted) ? "Ожидаем ответов" : "Группа собрана" : active.state === "queuing" ? "Запуск поиска" : "Поиск игры"}</Badge></div>
          {!validActiveSize && <p role="status" className="text-muted-foreground">{active.queueId === 440 ? "Для Flex нужно 1, 2, 3 или 5 игроков." : "Состав группы не подходит для этой очереди."}</p>}
          <div className="flex flex-col gap-2">{active.members.map(member => {
            const mine = member.id === snapshot?.me;
            return <div key={member.id} className="flex flex-wrap items-center gap-3 rounded-lg border p-2">
              <Avatar className="size-9"><AvatarImage src={profileIconUrl(member.profileIconId || 0) || undefined} alt="" /><AvatarFallback>{member.name.slice(0, 1)}</AvatarFallback></Avatar>
              <div className="min-w-0 flex-1 basis-36">
                <div className="flex items-center gap-1"><span className="truncate">{member.name}</span>{member.id === active.leaderId && <Crown className="size-3 shrink-0" aria-label="Лидер" />}{mine && <span className="text-muted-foreground">(вы)</span>}</div>
                <div className="flex flex-wrap items-center gap-2"><span className="truncate text-muted-foreground">{member.gameRiotId || "Игровой аккаунт не подключён"}</span><Badge variant="outline">{member.server || "Сервер —"}</Badge>{member.rankDisplay ? <RankBadge display={member.rankDisplay} /> : <span className="text-muted-foreground">Ранг —</span>}</div>
                <p className="text-[11px] text-muted-foreground">{member.online ? "В сети" : "Не в сети"} · {member.accepted ? "В группе" : "Приглашён"}</p>
              </div>
              {[400,420,440].includes(active.queueId) && <div className="flex flex-wrap items-center justify-end gap-2" aria-label={`Роли: ${member.name}`}>
                {([1,2] as const).filter(position => position === 1 || member.role !== "fill").map(position => <div key={position} role="group" aria-label={`${position === 1 ? "Основная" : "Вторая"} роль: ${member.name}`} className="flex items-center gap-1">
                  <span className="px-1 text-[10px] text-muted-foreground">{position}</span>
                  {(mine ? roles.filter(role => role.id && (position === 1 || role.id !== "fill")) : roles.filter(role => role.id === (position === 1 ? member.role || "fill" : member.secondaryRole))).map(role => <WithTooltip key={role.id} label={`${position === 1 ? "Основная" : "Вторая"}: ${role.name}`}>
                    <Button size="icon-sm" variant={(position === 1 ? member.role : member.secondaryRole) === role.id ? "default" : "outline"}
                      aria-label={`${position === 1 ? "Основная" : "Вторая"}: ${role.name}`} aria-pressed={(position === 1 ? member.role : member.secondaryRole) === role.id}
                      disabled={!mine || disabled || !connected || !member.accepted || active.state !== "forming" || position === 2 && role.id === member.role}
                      onClick={() => { if (mine) void run(() => partyAction(active.id, { expectedRevision: active.revision, action: "update", ...choosePartyRole(member.role,member.secondaryRole,role.id,position) })); }}>
                      {role.id && role.id !== "fill" ? <LolRoleIcon role={role.id} /> : <Shuffle className="size-4" />}
                    </Button>
                  </WithTooltip>)}
                </div>)}
              </div>}
            </div>;
          })}</div>
          {roleIssue && <p role="status" className="text-xs text-amber-500">{roleIssue}</p>}
          {snapshot?.local.matchingInvitation && own?.accepted && <Button size="sm" variant="outline" disabled={disabled || !connected} onClick={() => void run(() => partyAcceptLobby(active.id))}><LogIn data-icon="inline-start" />Принять приглашение в лобби</Button>}
          <div className="flex flex-wrap gap-2">
            {leader && <>
              <WithTooltip label="Создать лобби и пригласить участников"><Button size="sm" variant="outline" disabled={disabled || !connected || active.state !== "forming"} onClick={() => void run(() => partyLobby(active.id))}><Users data-icon="inline-start" />Лобби</Button></WithTooltip>
              {active.state === "forming" ? <WithTooltip label="Начать поиск с участниками лобби"><Button size="sm" disabled={disabled || !connected || !validActiveSize || !!roleIssue || !active.members.every(m => m.accepted && m.online)} onClick={() => { if (validActiveSize) void run(() => partyQueue(active.id, active.revision)); }}><Play data-icon="inline-start" />Найти игру</Button></WithTooltip>
                : <Button size="sm" variant="outline" disabled={disabled || !connected} onClick={() => void run(() => partyCancelQueue(active.id))}><Square data-icon="inline-start" />Отменить поиск</Button>}
              <Button size="sm" variant="destructive" disabled={disabled || active.state !== "forming"} onClick={() => void act(active, "cancel")}><X data-icon="inline-start" />Закрыть группу</Button>
            </>}
            {!leader && <Button size="sm" variant="outline" disabled={disabled} onClick={() => void act(active, "leave")}><LogOut data-icon="inline-start" />Выйти</Button>}
          </div>
        </section>}
      </CardContent></Card>
    </div>
  );
}
