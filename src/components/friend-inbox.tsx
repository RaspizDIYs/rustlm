"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Mail, RefreshCw, Trash2, UserPlus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import {
  goodluckAcceptFriendRequest,
  goodluckDeclineFriendRequest,
  goodluckGetFriendRequests,
  goodluckGetFriends,
  goodluckRemoveFriend,
  goodluckSearchUsers,
  goodluckSendFriendRequest,
  type GoodLuckFriend,
  type GoodLuckFriendRequest,
} from "@/lib/tauri";

/**
 * Конверт в сайдбаре: входящие заявки в друзья GoodLuck и управление друзьями.
 * Заявки принимаются прямо здесь, не уходя на сайт.
 */
export function FriendInbox({ collapsed = false }: { collapsed?: boolean }) {
  const [open, setOpen] = useState(false);
  const [requests, setRequests] = useState<GoodLuckFriendRequest[]>([]);
  const [friends, setFriends] = useState<GoodLuckFriend[]>([]);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<GoodLuckFriend[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const reload = useCallback(async () => {
    try {
      const [incoming, list] = await Promise.all([
        goodluckGetFriendRequests(),
        goodluckGetFriends(),
      ]);
      setRequests(incoming);
      setFriends(list);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
    const timer = setInterval(() => void reload(), 60000);
    let unlisten: (() => void) | undefined;
    void (async () => {
      const { isTauri } = await import("@tauri-apps/api/core");
      if (!isTauri()) return;
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen("cloud-sync-complete", () => void reload());
    })();
    return () => {
      clearInterval(timer);
      unlisten?.();
    };
  }, [reload]);

  useEffect(() => {
    if (searchTimer.current) clearTimeout(searchTimer.current);
    const trimmed = query.trim();
    if (trimmed.length < 2) {
      setResults([]);
      return;
    }
    searchTimer.current = setTimeout(() => {
      goodluckSearchUsers(trimmed)
        .then(setResults)
        .catch(() => setResults([]));
    }, 300);
    return () => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [query]);

  const run = useCallback(
    async (action: () => Promise<unknown>, success: string) => {
      setBusy(true);
      try {
        await action();
        toast.success(success);
        await reload();
      } catch (e) {
        toast.error(String(e));
      } finally {
        setBusy(false);
      }
    },
    [reload]
  );

  return (
    <>
      <button
        type="button"
        title="Заявки в друзья и друзья GoodLuck"
        onClick={() => {
          setOpen(true);
          void reload();
        }}
        className={cn(
          "relative flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs text-sidebar-foreground transition-colors hover:bg-accent hover:text-foreground",
          collapsed && "justify-center px-0"
        )}
      >
        <Mail className="h-4 w-4 shrink-0" />
        {!collapsed && <span className="flex-1 text-left">Друзья</span>}
        {requests.length > 0 && (
          <span className="absolute right-1 top-0.5 min-w-4 rounded-full bg-destructive px-1 text-center text-[9px] font-bold leading-4 text-destructive-foreground">
            {requests.length}
          </span>
        )}
      </button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Mail className="h-4 w-4" />
              Друзья GoodLuck
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-4">
            {error && <p className="text-xs text-destructive">{error}</p>}

            <section className="space-y-2">
              <div className="flex items-center justify-between">
                <p className="text-xs font-medium text-muted-foreground">
                  Заявки ({requests.length})
                </p>
                <Button
                  size="icon-sm"
                  variant="outline"
                  aria-label="Обновить заявки"
                  disabled={busy}
                  onClick={() => void reload()}
                >
                  <RefreshCw className={cn("h-3.5 w-3.5", busy && "animate-spin")} />
                </Button>
              </div>
              {requests.length === 0 ? (
                <p className="text-xs text-muted-foreground">Новых заявок нет.</p>
              ) : (
                requests.map((request) => (
                  <div key={request.id} className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate text-sm">
                      {request.requester_display_name || request.requester_username}
                    </span>
                    <Button
                      size="sm"
                      disabled={busy}
                      onClick={() =>
                        void run(
                          () => goodluckAcceptFriendRequest(request.id),
                          "Заявка принята"
                        )
                      }
                    >
                      Принять
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={busy}
                      onClick={() =>
                        void run(
                          () => goodluckDeclineFriendRequest(request.id),
                          "Заявка отклонена"
                        )
                      }
                    >
                      Отклонить
                    </Button>
                  </div>
                ))
              )}
            </section>

            <section className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground">Добавить друга</p>
              <Input
                placeholder="Имя или ник в GoodLuck (от 2 символов)"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {results.length > 0 && (
                <div className="max-h-40 space-y-1 overflow-y-auto">
                  {results.map((user) => (
                    <div key={user.id} className="flex items-center gap-2">
                      <span className="min-w-0 flex-1 truncate text-sm">
                        {user.display_name || user.username}
                      </span>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        aria-label={`Добавить ${user.display_name}`}
                        disabled={busy}
                        onClick={() =>
                          void run(
                            () => goodluckSendFriendRequest(user.id),
                            "Заявка отправлена"
                          )
                        }
                      >
                        <UserPlus className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  ))}
                </div>
              )}
            </section>

            <section className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground">
                Друзья ({friends.length})
              </p>
              {friends.length === 0 ? (
                <p className="text-xs text-muted-foreground">Пока никого нет.</p>
              ) : (
                <div className="max-h-48 space-y-1 overflow-y-auto">
                  {friends.map((friend) => (
                    <div key={friend.id} className="flex items-center gap-2">
                      <span className="min-w-0 flex-1 truncate text-sm">
                        {friend.display_name || friend.username}
                      </span>
                      <Button
                        size="icon-sm"
                        variant="ghost"
                        aria-label={`Удалить ${friend.display_name}`}
                        disabled={busy}
                        onClick={() =>
                          void run(
                            () => goodluckRemoveFriend(friend.id),
                            "Удалён из друзей"
                          )
                        }
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  ))}
                </div>
              )}
            </section>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
