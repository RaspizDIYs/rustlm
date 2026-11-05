"use client";
import { LeagueConnection, type LeagueSharingState } from "@/components/league-sharing";
import { HighlightText } from "@/components/highlight-text";
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  RefreshCw,
  ListRestart,
  Send,
  Cloud,
  Swords,
  Users,
  Trash2,
} from "lucide-react";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectGroup,
  SelectItem,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { WithTooltip } from "@/components/ui/with-tooltip";
import { Table, TableBody, TableRow, TableCell } from "@/components/ui/table";
import { Avatar, AvatarImage, AvatarFallback } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { friendsError, mergeFriends } from "@/lib/friends";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { goodluckRemoveFriend } from "@/lib/tauri";
import type { AccountRecord, GoodLuckFriend } from "@/lib/tauri";
export type LmFriend = GoodLuckFriend & {
  accounts: string[];
  riot_accounts?: { puuid: string; riot_id: string }[];
  updated_at: string;
};
export function LmFriends({
  accounts,
  league,
  onShare,
  query = "",
}: {
  accounts: AccountRecord[];
  league: LeagueSharingState;
  query?: string;
  onShare: (account: AccountRecord, friend: GoodLuckFriend) => void;
}) {
  const [gl, setGl] = useState<GoodLuckFriend[]>([]);
  const [target, setTarget] = useState("");
  const [glErrors, setGlErrors] = useState<string[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [removing, setRemoving] = useState<GoodLuckFriend | null>(null);
  const [removeBusy, setRemoveBusy] = useState(false);
  const [removeError, setRemoveError] = useState("");
  const enabled = league.visible;
  const busy = league.busy;
  const currentAccount = league.status?.puuid ? { puuid: league.status.puuid, riot_id: league.status.riot_id || "" } : null;
  const errors = [...glErrors, ...(league.error ? [league.error] : [])];
  const requests = useRef(0);
  const reload = useCallback(async () => {
    const request = ++requests.current;
    try {
      const { goodluckGetUser, goodluckGetFriends } = await import("@/lib/tauri");
      const user = await goodluckGetUser();
      const friends = user ? await goodluckGetFriends() : [];
      if (request !== requests.current) return;
      setGl(friends);
      setGlErrors([]);
    } catch (failure) { if (request === requests.current) setGlErrors([friendsError(failure)]); }
    finally { if (request === requests.current) setLoaded(true); }
  }, []);
  useEffect(() => {
    void reload();
    const refresh = () => void reload();
    window.addEventListener("rustlm-friends-changed", refresh);
    return () => { ++requests.current; window.removeEventListener("rustlm-friends-changed", refresh); };
  }, [reload]);
  const update = () => league.run(async () => {});
  const selected = accounts.find((a) => a.Username === target);
  const visible = mergeFriends(
    gl,
    league.friends,
  ).filter((friend) =>
    [
      friend.display_name,
      ...(friend.riot_accounts || []).map((a) => a.riot_id),
    ].some((value) =>
      value.toLocaleLowerCase("ru").includes(query.toLocaleLowerCase("ru")),
    ),
  );
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="flex min-h-0 flex-1 flex-col gap-3">
        <div className="friends-toolbar shrink-0">
          <div className="friends-toolbar-filters">
            <Select
              value={target || null}
              onValueChange={(value) => setTarget(value || "")}
            >
              <SelectTrigger size="sm" aria-label="Аккаунт для передачи">
                <SelectValue placeholder="Аккаунт для передачи">
                  {selected ? selected.RiotId || selected.Username : null}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {accounts
                    .filter((a) => a.EncryptedPassword)
                    .map((a) => (
                      <SelectItem key={a.Username} value={a.Username}>
                        {a.RiotId || a.Username}
                      </SelectItem>
                    ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </div>{" "}
          <div className="friends-toolbar-controls">
            <div className="flex items-center gap-2 text-xs">
              <span>Друзья League</span>
            <WithTooltip label="Разрешить друзьям League находить вас в LM">
              <Switch
                size="sm"
                aria-label="Обнаружение друзей LM"
                checked={enabled}
                disabled={busy || !league.status?.connected}
                onCheckedChange={async (next) => {
                  await league.run(() => league.reload(next));
                }}
              />
            </WithTooltip>
            </div>
            <div className="flex items-center gap-2">
            <WithTooltip label="Прочитать друзей из открытого клиента League">
              <Button
                aria-label="Обновить друзей League"
                size="icon-sm"
                variant="outline"
                disabled={busy || !enabled}
                onClick={() => void update()}
              >
                <RefreshCw className={busy ? "animate-spin" : undefined} />
              </Button>
            </WithTooltip>
            <WithTooltip label="Обновить списки LM и GoodLuck">
              <Button
                aria-label="Обновить списки друзей"
                size="icon-sm"
                variant="outline"
                disabled={busy}
                onClick={async () => {
                  await Promise.all([reload(), update()]);
                }}
              >
                <ListRestart />
              </Button>
            </WithTooltip>
            </div>
          </div>
        </div>
        <LeagueConnection league={league} />
        <p className="text-xs text-muted-foreground">
          {!league.status?.connected ? "Подключите League — вход в GoodLuck не нужен."
            : !league.status.matches_current ? `LM: ${league.status.owner_riot_id} · сохранённые друзья доступны на любом аккаунте League.`
            : !enabled ? "Включите «Друзья League», чтобы находить друзей с LM."
            : currentAccount ? `League: ${currentAccount.riot_id || "текущий аккаунт"} · только друзья с LM.`
            : "Откройте League и войдите в аккаунт, чтобы увидеть его друзей с LM."}
        </p>
        {errors.map((error, i) => (
          <p role="alert" className="text-xs text-destructive" key={i}>
            {error}
          </p>
        ))}
        {!loaded ? (
          <Skeleton className="h-16 w-full" />
        ) : visible.length ? (
          <div className="min-h-0 overflow-auto rounded-xl border">
            <Table>
              <TableBody>
                {visible.map((friend) => (
                  <TableRow key={friend.id}>
                    <TableCell>
                      <div className="flex items-center gap-2.5">
                        <Avatar className="size-8">
                          <AvatarImage
                            src={friend.avatar_url || undefined}
                            alt={friend.display_name}
                          />
                          <AvatarFallback>
                            {Array.from(friend.display_name.trim())
                              .slice(0, 2)
                              .join("")
                              .toUpperCase() || "?"}
                          </AvatarFallback>
                        </Avatar>
                        <div className="min-w-0">
                          <div className="flex flex-wrap items-center gap-2">
                            <span className="truncate font-medium">
                              <HighlightText text={friend.display_name} query={query} />
                            </span>
                            {friend.sources.includes("riot") && (
                              <WithTooltip label="Друг из League">
                                <Badge
                                  variant="outline"
                                  className="gap-1 px-1.5 text-[10px]"
                                >
                                  <Swords />
                                  League
                                </Badge>
                              </WithTooltip>
                            )}
                            {friend.sources.includes("goodluck") && (
                              <WithTooltip label="Друг из GoodLuck">
                                <Badge
                                  variant="outline"
                                  className="gap-1 px-1.5 text-[10px]"
                                >
                                  <Cloud />
                                  GoodLuck
                                </Badge>
                              </WithTooltip>
                            )}
                          </div>
                          {friend.riot_accounts?.filter((a) => a.riot_id !== friend.display_name).map((a) => (
                            <p
                              className="truncate text-xs text-muted-foreground"
                              key={a.puuid}
                            >
                              <HighlightText text={a.riot_id} query={query} />
                            </p>
                          ))}
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className="w-20">
                      <div className="flex justify-end gap-1">
                      <WithTooltip
                        label={
                          selected
                            ? "Передать выбранный аккаунт"
                            : "Сначала выберите аккаунт для передачи"
                        }
                      >
                        <Button
                          size="icon-sm"
                          variant="outline"
                          aria-label={`Передать аккаунт: ${friend.display_name}`}
                          disabled={!selected?.EncryptedPassword}
                          onClick={() => { if (selected) { onShare(selected, friend); } }}
                        >
                          <Send />
                        </Button>
                      </WithTooltip>
                      {(friend.sources.includes("goodluck") || friend.id.startsWith("league:")) && (
                        <WithTooltip label="Удалить из друзей LM">
                          <Button size="icon-sm" variant="ghost" aria-label={`Удалить друга: ${friend.display_name}`} onClick={() => { setRemoveError(""); setRemoving(friend); }}>
                            <Trash2 className="size-4 text-destructive" />
                          </Button>
                        </WithTooltip>
                      )}
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        ) : (
          <div
            role="status"
            className="flex items-center justify-center gap-2 py-8 text-sm text-muted-foreground"
          >
            <Users className="size-4" />
            Друзья пока не найдены
          </div>
        )}
      </div>
      <Dialog open={Boolean(removing)} onOpenChange={(open) => { if (!open && !removeBusy) setRemoving(null); }}>
        <DialogContent className="accounts-dialog max-w-sm">
          <DialogHeader>
            <DialogTitle className="text-sm">Удалить друга?</DialogTitle>
            <DialogDescription>{removing?.display_name} будет удалён из друзей LM, доступ к переданным друг другу аккаунтам будет закрыт; дружба в League сохранится.</DialogDescription>
          </DialogHeader>
          {removeError && <p role="alert" className="text-xs text-destructive">{removeError}</p>}
          <DialogFooter>
            <Button size="sm" variant="outline" disabled={removeBusy} onClick={() => setRemoving(null)}>Отмена</Button>
            <Button size="sm" variant="destructive" disabled={removeBusy} onClick={async () => {
              if (!removing) return;
              setRemoveBusy(true);
              setRemoveError("");
              try {
                if (removing.id.startsWith("league:")) { await invoke("league_remove_friend", { puuid: removing.id.slice(7) }); await league.reload(); }
                else await goodluckRemoveFriend(removing.id);
                setGl((friends) => friends.filter((friend) => friend.id !== removing.id));
                setRemoving(null);
                await reload();
              } catch {
                setRemoveError("Не удалось удалить друга. Проверьте подключение и повторите попытку.");
              } finally { setRemoveBusy(false); }
            }}>{removeBusy ? "Удаляем…" : "Удалить"}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
