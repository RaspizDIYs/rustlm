"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  WandSparkles,
  RefreshCw,
  MonitorOff,
  Monitor,
  RotateCw,
  Loader2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { WithTooltip } from "@/components/ui/with-tooltip";
import {
  UNGLITCH_ACTIONS,
  actionAvailable,
  confirmationRequired,
  getUnglitchStatus,
  executeUnglitch,
  type UnglitchAction,
  type UnglitchStatus,
} from "@/lib/unglitch";

const icons = {
  reload: RefreshCw,
  force_reload: RotateCw,
  hide: MonitorOff,
  restore: Monitor,
};
const phases: Record<string, string> = {
  None: "Ожидание",
  Lobby: "Лобби",
  Matchmaking: "Поиск игры",
  ReadyCheck: "Подтверждение игры",
  ChampSelect: "Выбор чемпионов",
  InProgress: "Матч",
  Reconnect: "Переподключение",
  EndOfGame: "Матч завершён",
  PreEndOfGame: "Завершение матча",
};
export default function UnglitchPage() {
  const [status, setStatus] = useState<UnglitchStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [busy, setBusy] = useState<UnglitchAction | null>(null);
  const [confirm, setConfirm] = useState<UnglitchAction | null>(null);
  const [error, setError] = useState("");
  const [statusError, setStatusError] = useState("");
  const [message, setMessage] = useState("");
  const mounted = useRef(true),
    pending = useRef(false),
    reading = useRef(false),
    revision = useRef(0);
  const refresh = useCallback(async (showSpinner = false) => {
    if (reading.current || pending.current) return;
    reading.current = true;
    const started = revision.current;
    if (showSpinner) setChecking(true);
    try {
      const next = await getUnglitchStatus();
      if (mounted.current && started === revision.current) {
        setStatus(next);
        setStatusError("");
      }
    } catch {
      if (mounted.current && started === revision.current)
        setStatusError(
          "Не удалось проверить клиент League. Повторите проверку.",
        );
    } finally {
      reading.current = false;
      if (mounted.current) setChecking(false);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void refresh();
    const visible = () => {
      if (!document.hidden) void refresh();
    };
    document.addEventListener("visibilitychange", visible);
    const timer = setInterval(visible, 5000);
    return () => {
      mounted.current = false;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [refresh]);
  const run = async (action: UnglitchAction, confirmed = false) => {
    if (pending.current) return;
    if (!confirmed && confirmationRequired(action, status?.phase ?? null)) {
      setConfirm(action);
      return;
    }
    pending.current = true;
    ++revision.current;
    setBusy(action);
    setConfirm(null);
    setError("");
    setMessage("");
    try {
      const result = await executeUnglitch(action, confirmed);
      if (mounted.current) {
        setStatus(result.status);
        setMessage(result.message);
      }
    } catch (cause) {
      if (mounted.current) {
        const detail = cause instanceof Error ? cause.message : String(cause);
        if (detail.includes("CONFIRM_REQUIRED")) setConfirm(action);
        else setError(detail);
      }
    } finally {
      pending.current = false;
      if (mounted.current) {
        setBusy(null);
        void refresh();
      }
    }
  };
  const confirmation = UNGLITCH_ACTIONS.find((item) => item.id === confirm);
  return (
    <div className="flex max-w-4xl flex-col gap-3">
      <header className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <WandSparkles className="size-5 text-primary" />
          <h1 className="text-lg font-semibold">Unglitch</h1>
        </div>
        <WithTooltip label="Обновить состояние">
          <Button
            size="icon-sm"
            variant="outline"
            aria-label="Обновить состояние клиента"
            disabled={checking || busy !== null}
            onClick={() => {
              setError("");
              void refresh(true);
            }}
          >
            <RefreshCw className={checking ? "animate-spin" : undefined} />
          </Button>
        </WithTooltip>
      </header>
      <p className="text-xs text-muted-foreground">
        Восстановление интерфейса League без полного перезапуска клиента.
      </p>
      <div
        className="flex flex-wrap items-center gap-2"
        aria-label="Состояние League"
        aria-live="polite"
      >
        <Badge variant="outline">
          League:{" "}
          {status
            ? status.backend_running
              ? "запущен"
              : "не запущен"
            : "проверяем"}
        </Badge>
        <Badge variant="outline">
          Интерфейс:{" "}
          {status ? (status.ui_running ? "открыт" : "закрыт") : "проверяем"}
        </Badge>
        <Badge variant="outline">
          Связь:{" "}
          {status ? (status.api_available ? "есть" : "нет") : "проверяем"}
        </Badge>
        {status?.phase && (
          <Badge variant="secondary">
            {phases[status.phase] || "Клиент работает"}
          </Badge>
        )}
      </div>
      {(error || statusError) && (
        <p
          role="alert"
          className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive"
        >
          {error || statusError}
        </p>
      )}
      {message && (
        <p
          role="status"
          className="rounded-lg border border-primary/30 bg-primary/10 p-3 text-xs"
        >
          {message}
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        {UNGLITCH_ACTIONS.map((item) => {
          const Icon = icons[item.id];
          return (
            <Card key={item.id}>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Icon className="size-4" />
                  {item.title}
                </CardTitle>
              </CardHeader>
              <CardContent className="flex flex-1 flex-col items-start justify-between gap-3">
                <p className="text-xs text-muted-foreground">
                  {item.description}
                </p>
                <Button
                  className="w-40"
                  variant={item.id === "reload" ? "default" : "outline"}
                  disabled={
                    checking ||
                    busy !== null ||
                    !actionAvailable(item.id, status)
                  }
                  onClick={() => void run(item.id)}
                >
                  {busy === item.id ? (
                    <Loader2 className="animate-spin" />
                  ) : (
                    <Icon />
                  )}
                  {busy === item.id ? "Выполняем…" : item.button}
                </Button>
              </CardContent>
            </Card>
          );
        })}
      </div>
      <Dialog
        open={confirm !== null}
        onOpenChange={(open) => {
          if (!open) setConfirm(null);
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{confirmation?.title}</DialogTitle>
            <DialogDescription>
              {confirm === "hide"
                ? "Окно League закроется; вернуть его можно кнопкой «Открыть окно» в LM. Сохранение текущей сессии не гарантируется."
                : "Действие может повлиять на текущий поиск или выбор чемпионов; сохранение сессии не гарантируется."}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirm(null)}>
              Отмена
            </Button>
            <Button
              onClick={() => {
                if (confirm) void run(confirm, true);
              }}
            >
              Продолжить
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
