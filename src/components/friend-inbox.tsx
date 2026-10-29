"use client";

import { useCallback, useEffect, useState } from "react";
import { Mail, RefreshCw, Check, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Avatar, AvatarImage, AvatarFallback } from "@/components/ui/avatar";
import { WithTooltip } from "@/components/ui/with-tooltip";
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
  type GoodLuckFriendRequest,
} from "@/lib/tauri";

export function FriendInbox({ collapsed = false }: { collapsed?: boolean }) {
  const [open, setOpen] = useState(false);
  const [requests, setRequests] = useState<GoodLuckFriendRequest[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setRequests(await goodluckGetFriendRequests());
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

  const run = useCallback(
    async (action: () => Promise<unknown>, success: string) => {
      setBusy(true);
      try {
        await action();
        toast.success(success);
        window.dispatchEvent(new Event("rustlm-friends-changed"));
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
        title="Заявки в друзья"
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
        {!collapsed && <span className="flex-1 text-left">Заявки</span>}
        {requests.length > 0 && (
          <span className="absolute right-1 top-0.5 min-w-4 rounded-full bg-destructive px-1 text-center text-[9px] font-bold leading-4 text-destructive-foreground">
            {requests.length}
          </span>
        )}
      </button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="accounts-dialog max-w-md gap-3">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2 text-sm">
              <Mail className="h-4 w-4" />
              Заявки в друзья
            </DialogTitle>
          </DialogHeader>

          <div className="max-h-80 space-y-3 overflow-y-auto">
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
                  <div key={request.id} className="flex items-center gap-2 rounded-lg border px-3 py-2">
                    <Avatar className="size-8"><AvatarImage src={request.requester_avatar_url || undefined} alt="" /><AvatarFallback>{(request.requester_display_name || request.requester_username).slice(0, 2).toUpperCase()}</AvatarFallback></Avatar>
                    <span className="min-w-0 flex-1 truncate text-sm">
                      {request.requester_display_name || request.requester_username}
                    </span>
                    <WithTooltip label="Принять заявку"><Button
                      size="icon-sm"
                      aria-label="Принять заявку"
                      disabled={busy}
                      onClick={() =>
                        void run(
                          () => goodluckAcceptFriendRequest(request.id),
                          "Заявка принята"
                        )
                      }
                    >
                      <Check className="size-4" />
                    </Button></WithTooltip>
                    <WithTooltip label="Отклонить заявку"><Button
                      size="icon-sm"
                      aria-label="Отклонить заявку"
                      variant="outline"
                      disabled={busy}
                      onClick={() =>
                        void run(
                          () => goodluckDeclineFriendRequest(request.id),
                          "Заявка отклонена"
                        )
                      }
                    >
                      <X className="size-4" />
                    </Button></WithTooltip>
                  </div>
                ))
              )}
            </section>

          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
