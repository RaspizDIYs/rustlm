"use client";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  RefreshCw,
  ListRestart,
  Send,
  Cloud,
  Swords,
  Users,
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
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { friendsError, mergeFriends } from "@/lib/friends";
import type { AccountRecord, GoodLuckFriend } from "@/lib/tauri";
export type LmFriend = GoodLuckFriend & {
  accounts: string[];
  riot_accounts?: { puuid: string; riot_id: string }[];
  updated_at: string;
};
export function LmFriends({
  accounts,
  onShare,
  query = "",
}: {
  accounts: AccountRecord[];
  query?: string;
  onShare: (account: AccountRecord, friend: GoodLuckFriend) => void;
}) {
  const [gl, setGl] = useState<GoodLuckFriend[]>([]),
    [lm, setLm] = useState<LmFriend[]>([]),
    [scope, setScope] = useState("all"),
    [target, setTarget] = useState(""),
    [enabled, setEnabled] = useState(false),
    [busy, setBusy] = useState(false),
    [errors, setErrors] = useState<string[]>([]),
    [loaded, setLoaded] = useState(false);
  const reload = useCallback(async () => {
    const { goodluckGetFriends, loadSetting } = await import("@/lib/tauri");
    const results = await Promise.allSettled([
      goodluckGetFriends(),
      invoke<LmFriend[]>("lm_friends"),
      loadSetting("LmFriendsVisible", false),
    ]);
    if (results[0].status === "fulfilled") setGl(results[0].value);
    if (results[1].status === "fulfilled") setLm(results[1].value);
    if (results[2].status === "fulfilled")
      setEnabled(Boolean(results[2].value));
    setErrors([
      ...new Set(
        results.flatMap((r) =>
          r.status === "rejected" ? [friendsError(r.reason)] : [],
        ),
      ),
    ]);
    setLoaded(true);
  }, []);
  useEffect(() => {
    void reload();
  }, [reload]);
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false,
      running = false;
    const refresh = async () => {
      if (running) return;
      running = true;
      try {
        await invoke("lm_publish_friends");
        if (!cancelled) {
          await reload();
        }
      } catch (error) {
        if (!cancelled) {
          setErrors([friendsError(error)]);
        }
      } finally {
        running = false;
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 60000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [enabled, reload]);
  const update = async () => {
    setBusy(true);
    try {
      await invoke("lm_publish_friends");
      await reload();
    } catch (e) {
      setErrors([friendsError(e)]);
    } finally {
      setBusy(false);
    }
  };
  const selected = accounts.find((a) => a.Username === target);
  const visible = mergeFriends(
    gl,
    lm.filter((f) => scope === "all" || f.accounts.includes(scope)),
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
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <Select
              value={scope}
              onValueChange={(value) => value && setScope(value)}
            >
              <SelectTrigger size="sm" aria-label="Друзья аккаунта">
                <SelectValue>
                  {scope === "all"
                    ? "Все аккаунты"
                    : accounts.find((a) => a.Puuid === scope)?.RiotId ||
                      "Аккаунт"}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="all">Все аккаунты</SelectItem>
                  {accounts
                    .filter((a) => a.Puuid)
                    .map((a) => (
                      <SelectItem key={a.Username} value={a.Puuid!}>
                        {a.RiotId || a.Username}
                      </SelectItem>
                    ))}
                </SelectGroup>
              </SelectContent>
            </Select>
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
          <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
            <WithTooltip label="Разрешить Riot-друзьям находить вас в LM">
              <Switch
                size="sm"
                aria-label="Обнаружение друзей LM"
                checked={enabled}
                disabled={busy}
                onCheckedChange={async (next) => {
                  setBusy(true);
                  try {
                    await invoke("lm_set_visibility", { enabled: next });
                    setEnabled(next);
                    await reload();
                  } catch (error) {
                    setErrors([friendsError(error)]);
                  } finally {
                    setBusy(false);
                  }
                }}
              />
            </WithTooltip>
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
                  setBusy(true);
                  try {
                    await reload();
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                <ListRestart />
              </Button>
            </WithTooltip>
          </div>
        </div>
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
                      <div className="flex items-center gap-2">
                        <span className="font-medium">
                          {friend.display_name}
                        </span>
                        {friend.sources.includes("riot") && (
                          <WithTooltip label="Друг из League">
                            <span
                              tabIndex={0}
                              aria-label="League"
                              className="text-muted-foreground"
                            >
                              <Swords className="size-3.5" />
                            </span>
                          </WithTooltip>
                        )}
                        {friend.sources.includes("goodluck") && (
                          <WithTooltip label="Друг из GoodLuck">
                            <span
                              tabIndex={0}
                              aria-label="GoodLuck"
                              className="text-muted-foreground"
                            >
                              <Cloud className="size-3.5" />
                            </span>
                          </WithTooltip>
                        )}
                      </div>
                      {friend.riot_accounts?.map((a) => (
                        <p
                          className="text-xs text-muted-foreground"
                          key={a.puuid}
                        >
                          {a.riot_id}
                        </p>
                      ))}
                    </TableCell>
                    <TableCell className="w-10">
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
                          onClick={() => selected && onShare(selected, friend)}
                        >
                          <Send />
                        </Button>
                      </WithTooltip>
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
    </div>
  );
}
