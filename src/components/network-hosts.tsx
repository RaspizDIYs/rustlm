"use client";
import { RepairError } from "@/components/repair-error";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  applyHosts,
  previewHosts,
  restoreHosts,
  type HostsPlan,
  type NetworkReport,
} from "@/lib/network";

export function NetworkHosts({
  report,
  disabled,
}: {
  report: NetworkReport | undefined;
  disabled: boolean;
}) {
  const [selected, setSelected] = useState<Record<string, string>>({});
  const [plan, setPlan] = useState<HostsPlan | null>(null);
  const [backup, setBackup] = useState("");
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    setSelected({});
    setPlan(null);
  }, [report?.id]);
  useEffect(() => {
    setBackup(localStorage.getItem("rustlm-hosts-backup") ?? "");
  }, []);
  const candidates = new Map<string, Set<string>>();
  if (report && report.context.connection !== "vpn" && !report.cancelled) {
    for (const p of report.probes) {
      if (
        p.classification === "http_success" &&
        p.tls &&
        p.port === 443 &&
        !p.ip.includes(":") &&
        /(^|\.)(riotgames\.com|riotcdn\.net|leagueoflegends\.com|pvp\.net|rgpub\.io)$/.test(
          p.domain,
        )
      ) {
        if (!candidates.has(p.domain)) candidates.set(p.domain, new Set());
        candidates.get(p.domain)!.add(p.ip);
      }
    }
  }
  const action = async (run: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await run();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <details>
      <summary className="cursor-pointer text-xs font-medium">
        Дополнительно: проверенные адреса в hosts
      </summary>
      <div className="mt-3 flex flex-col gap-3">
        <p className="text-xs text-muted-foreground">
          Необязательный шаг для проверки проблем DNS. Доступны только IPv4 с
          успешным HTTPS-ответом в выбранном замере без VPN. Фиксация CDN может
          устареть; при ухудшении выполните откат. Отказы Cloudflare и
          неподтверждённые адреса не предлагаются.
        </p>
        {error && <RepairError error={error} />}
        {message && (
          <p role="status" className="text-xs">
            {message}
          </p>
        )}
        {candidates.size === 0 ? (
          <p className="text-xs text-muted-foreground">
            Нет подходящих адресов. Выберите завершённый замер без VPN с
            успешными HTTPS-ответами.
          </p>
        ) : (
          <div className="flex max-h-60 flex-col gap-3 overflow-auto">
            {[...candidates].map(([domain, ips]) => (
              <div key={domain} className="flex flex-wrap items-center gap-3">
                <Switch
                  id={`pin-${domain}`}
                  checked={!!selected[domain]}
                  disabled={disabled || busy}
                  onCheckedChange={(checked) =>
                    setSelected((old) => {
                      const next = { ...old };
                      if (checked) next[domain] = [...ips][0];
                      else delete next[domain];
                      return next;
                    })
                  }
                />
                <label
                  htmlFor={`pin-${domain}`}
                  className="min-w-0 flex-1 break-all text-xs"
                >
                  {domain}
                </label>
                <Select
                  value={selected[domain] ?? [...ips][0]}
                  disabled={disabled || busy || !selected[domain]}
                  onValueChange={(ip) =>
                    ip && setSelected((old) => ({ ...old, [domain]: ip }))
                  }
                >
                  <SelectTrigger aria-label={`IP для ${domain}`}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {[...ips].map((ip) => (
                        <SelectItem key={ip} value={ip}>
                          {ip}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
            ))}
          </div>
        )}
        <div className="flex flex-wrap gap-3">
          <Button
            variant="outline"
            disabled={disabled || busy || !Object.keys(selected).length}
            onClick={() =>
              action(async () => {
                setPlan(
                  await previewHosts(
                    report!.id,
                    Object.entries(selected).map(([domain, ip]) => ({
                      domain,
                      ip,
                    })),
                  ),
                );
              })
            }
          >
            Показать изменения hosts
          </Button>
          {backup && (
            <Button
              variant="outline"
              disabled={disabled || busy}
              onClick={() => setRestoreOpen(true)}
            >
              Откатить hosts
            </Button>
          )}
        </div>
        <p className="text-xs text-muted-foreground">
          Для записи нужны права администратора. Windows запросит разрешение на
          изменение hosts. Другие записи сохраняются; существующий адрес домена
          автоматически не заменяется.
        </p>
      </div>
      <Dialog
        open={!!plan}
        onOpenChange={(open) => {
          if (!open && !busy) setPlan(null);
        }}
      >
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>Изменения системного hosts</DialogTitle>
            <DialogDescription>
              Добавится отдельный блок RustLM. Резервная копия создаётся до
              записи. Применяйте на том подключении, где выполнен замер.
            </DialogDescription>
          </DialogHeader>
          <pre className="max-h-80 overflow-auto rounded-md bg-muted p-3 text-xs">
            {plan?.after}
          </pre>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => setPlan(null)}
            >
              Отмена
            </Button>
            <Button
              disabled={busy}
              onClick={() =>
                action(async () => {
                  const id = await applyHosts(plan!);
                  setBackup(id);
                  localStorage.setItem("rustlm-hosts-backup", id);
                  setPlan(null);
                  setMessage("Блок hosts применён. Выполните повторный замер.");
                })
              }
            >
              Применить с резервной копией
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={restoreOpen} onOpenChange={setRestoreOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Восстановить hosts?</DialogTitle>
            <DialogDescription>
              Если hosts изменился после настройки, автооткат остановится для
              сохранения новых записей.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => setRestoreOpen(false)}
            >
              Отмена
            </Button>
            <Button
              disabled={busy}
              onClick={() =>
                action(async () => {
                  await restoreHosts(backup);
                  setBackup("");
                  localStorage.removeItem("rustlm-hosts-backup");
                  setRestoreOpen(false);
                  setMessage("Исходный hosts восстановлен.");
                })
              }
            >
              Восстановить
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </details>
  );
}
