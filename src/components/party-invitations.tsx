"use client";

import { serverCode } from "@/lib/lol-servers";
import { Users } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { profileIconUrl } from "@/lib/lol-assets";
import { GameModeIcon } from "@/components/game-mode-icon";
import { createPartyRequestGate, partyActionError } from "./party-dialog-request";
import { allowedSizes, partyAction, partyJoinWithAccount, partySnapshot, type PartySnapshot, type Party } from "@/lib/party";
import type { AccountRecord } from "@/lib/tauri";



export function PartyInvitations({ snapshot, accounts, onSnapshot }: {
  snapshot: PartySnapshot | null; accounts: AccountRecord[]; onSnapshot: (snapshot: PartySnapshot) => void;
}) {
  const [choices, setChoices] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [retained, setRetained] = useState<Party | null>(null);
  const gate = useRef(createPartyRequestGate());
  useEffect(() => () => gate.current.invalidate(), []);
  const run = async (party: Party, operation: () => Promise<unknown>) => {
    await gate.current.run(async current => {
      try { await operation(); if (current()) setRetained(null); }
      catch (failure) { if (current()) setError(partyActionError(failure)); }
      try {
        if (current()) {
          const next = await partySnapshot();
          if (current()) onSnapshot(next);
        }
      } catch { if (current()) setError("Не удалось обновить группу — проверьте подключение перед повтором."); }
      finally { if (current()) setBusy(null); }
    }, () => { setBusy(party.id); setRetained(party); setError(""); });
  };
  const invitations = snapshot?.invitations || [];
  const rows = retained && !invitations.some(party => party.id === retained.id) ? [...invitations, snapshot?.active?.id === retained.id ? snapshot.active : retained] : invitations;
  if (!rows.length && !busy && !error) return null;
  return <section aria-label="Приглашения в пати LM" className="flex max-h-72 shrink-0 flex-col gap-2 overflow-y-auto rounded-lg border p-3 text-xs">
    <p className="font-medium">Приглашения в пати LM</p>
    {error && <p role="alert" className="text-destructive">{error}</p>}
    {busy && <p role="status" className="text-muted-foreground">Выполняем действие… При подключении ждём приглашение League до 60 секунд.</p>}
    {rows.map(party => {
      const leader = party.members.find(member => member.id === party.leaderId);
      const server = serverCode(leader?.server || "");
      const eligible = accounts.filter(account => !!server && serverCode(account.Server) === server && !!account.EncryptedPassword.trim() && !!account.Username && !account.Username.startsWith("gl:"));
      const selected = eligible.find(account => account.Username === choices[party.id]);
      const ready = leader?.lobbyReady === true && (leader.lobbyCount ?? 0) > 0;
      return <div key={party.id} className="flex flex-wrap items-center gap-2 rounded-lg border p-2">
        <Avatar className="size-8"><AvatarImage src={profileIconUrl(leader?.profileIconId ?? 0) || undefined} alt="" /><AvatarFallback>{(leader?.name || "LM").slice(0, 1)}</AvatarFallback></Avatar>
        <div className="min-w-0 flex-1 basis-40">
          <p className="truncate font-medium">{leader?.name || "Друг LM"}</p>
          <p className="flex items-center gap-1 text-muted-foreground"><GameModeIcon mode={party.queueId} />{snapshot?.queues.find(queue => queue.id === party.queueId)?.name || `Очередь ${party.queueId}`} · {server || "Сервер неизвестен"}</p>
          <p className="flex items-center gap-1 text-muted-foreground"><Users aria-hidden className="size-3.5" /><span aria-label="Участники лобби">{leader?.lobbyCount ?? "—"}/{allowedSizes(party.queueId, snapshot?.queues.find(queue => queue.id === party.queueId)?.maxPlayers)?.at(-1) ?? "—"}</span>{!ready && " · Ожидаем лобби лидера"}</p>
        </div>
        <Select value={selected?.Username ?? null} disabled={!!busy || !eligible.length} onValueChange={value => setChoices(previous => ({ ...previous, [party.id]: value || "" }))}>
          <SelectTrigger aria-label={`Аккаунт для группы ${leader?.name || "LM"}`} className="w-48"><SelectValue placeholder="Выберите аккаунт">{selected ? selected.RiotId || selected.SummonerName || "Сохранённый аккаунт" : undefined}</SelectValue></SelectTrigger>
          <SelectContent><SelectGroup>{eligible.map((account, index) => <SelectItem key={account.Username} value={account.Username}>{account.RiotId || account.SummonerName || `Аккаунт ${index + 1}`} · {server}</SelectItem>)}</SelectGroup></SelectContent>
        </Select>
        <Button size="sm" disabled={!!busy || !ready || !selected} onClick={() => { if (selected && ready) void run(party, () => partyJoinWithAccount(party.id, selected.Username)); }}>Перезайти и присоединиться</Button>
        <Button size="sm" variant="outline" disabled={!!busy} onClick={() => void run(party, () => partyAction(party.id, { expectedRevision: party.revision, action: snapshot?.active?.id === party.id ? "leave" : "decline" }))}>Отклонить</Button>
        {!eligible.length && <p className="w-full text-muted-foreground">{server ? `Добавьте активированный аккаунт сервера ${server}.` : "Ожидаем сервер лидера."}</p>}
      </div>;
    })}
  </section>;
}
