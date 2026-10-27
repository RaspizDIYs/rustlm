"use client";
import Link from "next/link";
import {networkSummary,noIpv6Route,infrastructureDomain} from "@/lib/network-summary";
import { RepairError } from "@/components/repair-error";

import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { isTauri } from "@tauri-apps/api/core";
import {
  Loader2,
  FolderOpen,
  Play,
  Square,
  Undo2,
  Download,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { NetworkHosts } from "@/components/network-hosts";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  applyNetworkPatch,
  cancelNetworkRun,
  classificationLabels,
  compareReports,
  inspectZapret,
  loadNetworkHistory,
  previewNetworkPatch,
  restoreNetworkPatch,
  runNetwork,
  type Installation,
  type NetworkReport,
  type Patch,
  type Progress,
  type RunContext,
} from "@/lib/network";

const connectionLabels = {
  direct: "Без VPN и Zapret",
  vpn: "С VPN",
  zapret: "С Zapret",
};

function exportReport(report: NetworkReport) {
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(report, null, 2)], { type: "application/json" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = `rustlm-network-${report.id}.json`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export default function ConnectionPage() {
  const [installation, setInstallation] = useState<Installation | null>(null);
  const [strategy, setStrategy] = useState("");
  const provider = "Текущее подключение на этом ПК";
  const [region, setRegion] = useState<RunContext["region"]>("RU");
  const [connection, setConnection] =
    useState<RunContext["connection"]>("direct");
  const [reports, setReports] = useState<NetworkReport[]>([]);
  const [selected, setSelected] = useState("");
  const [comparison, setComparison] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [patch, setPatch] = useState<Patch | null>(null);
  const [backup, setBackup] = useState<{ root: string; id: string } | null>(
    null,
  );
  const [restoreConfirm, setRestoreConfirm] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [filter, setFilter] = useState("");
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    if (!isTauri()) {
      setError("Сетевые проверки доступны в десктопной версии RustLM.");
      return () => {
        mounted.current = false;
      };
    }
    void loadNetworkHistory()
      .then((history) => {
        if (mounted.current) {
          setReports(history);
          setSelected(history[0]?.id ?? "");
        }
      })
      .catch((e) => {
        if (mounted.current) setError(String(e));
      });
    try {
      const saved = localStorage.getItem("rustlm-network-backup");
      if (saved) setBackup(JSON.parse(saved));
    } catch {
      /* A missing/invalid local pointer never changes files. */
    }
    return () => {
      mounted.current = false;
    };
  }, []);

  const act = async (name: string, action: () => Promise<void>) => {
    setBusy(name);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (e) {
      setError(String(e));
    } finally {
      if (mounted.current) setBusy(null);
    }
  };
  const choose = () =>
    act("folder", async () => {
      const path = await open({
        directory: true,
        multiple: false,
        title: "Папка установленного Zapret",
      });
      if (typeof path !== "string") return;
      const info = await inspectZapret(path);
      setInstallation(info);
      setStrategy(info.strategies[0] ?? "");
      setPatch(null);
    });
  const start = () =>
    act("run", async () => {
      setProgress({
        phase: "dns",
        completed: 0,
        total: 0,
        current: "Подготовка",
      });
      const report = await runNetwork(
        {
          connection,
          provider: provider.trim(),
          region,
          strategy: connection === "zapret" ? strategy : null,
          strategy_hash: null,
        },
        installation?.root ?? null,
        (p) => {
          if (mounted.current) setProgress(p);
        },
      );
      if (mounted.current) {
        setReports((old) => [report, ...old].slice(0, 20));
        setSelected(report.id);
        setNotice(
          report.cancelled
            ? "Прогон остановлен, частичные результаты сохранены."
            : "Прогон завершён. Результаты сохранены.",
        );
      }
    });
  const report = reports.find((r) => r.id === selected);
  const summary = report ? networkSummary(report) : null;
  const before = reports.find((r) => r.id === comparison);
  const diff = report && before ? compareReports(before, report) : null;
  const probes =
    report?.probes.filter((p) =>
      `${p.domain} ${p.ip} ${classificationLabels[p.classification] ?? p.classification}`
        .toLowerCase()
        .includes(filter.toLowerCase()),
    ) ?? [];
  const dnsFailures = report
    ? [...new Set(report.dns.map((d) => d.domain))].filter(
        (domain) =>
          !report.dns.some(
            (d) => d.domain === domain && d.addresses.length > 0,
          ),
      )
    : [];

  return (
    <div className="repair-compact flex flex-col gap-2">
      <header className="flex flex-col gap-2">
        <h1 className="text-lg font-semibold">
          Подключение к League of Legends
        </h1>
        <p className="text-xs text-muted-foreground">
          Здесь объясняем сетевой замер. Для обычной настройки откройте мастер — он сам выполнит шаги.
        </p>
      </header>
      <Link href="/repair-tools" className="w-fit rounded border bg-primary px-3 py-1 text-xs text-primary-foreground">Открыть мастер восстановления</Link>
      {summary && <Card size="sm"><CardHeader><CardTitle>Что показал замер</CardTitle></CardHeader><CardContent className="space-y-2 text-xs">
        <p>{summary.responding} сервисов отвечают; {summary.restricted} отвечают с ограничением доступа; {summary.needsCheck.length} требуют дополнительной проверки.</p>
        <p className="text-muted-foreground">Считаем сервисы, а не отдельные IP: недоступный запасной адрес не означает поломку всего сервиса. Ответ 403/404 или Cloudflare — отдельный результат, не повод автоматически менять DNS.</p>
        {summary.ipv6Unavailable>0&&<p>IPv6 недоступен на этом ПК: {summary.ipv6Unavailable} проб исключены из оценки, когда есть проверка IPv4. Включать IPv6 ради исправления League не требуется.</p>}
        {summary.infrastructure>0&&<p>Служебные DNS-узлы CDN ({summary.infrastructure}) показаны только в подробностях: они направляют трафик, но не обязаны открываться как сайты.</p>}
        {summary.brokenSystemDns.length>0?<p>У {summary.brokenSystemDns.length} имён системный DNS не вернул адрес, а другие DNS вернули. Мастер проверит рабочий HTTPS и при подтверждении предложит исправление hosts.</p>:<p>Подтверждённой разницы между системным и внешними DNS не найдено. Добавлять весь каталог в hosts не нужно.</p>}
        {summary.unresolved.length>0&&<p>У {summary.unresolved.length} имён нет адресов во всех проверенных DNS. По этому замеру нельзя отличить неиспользуемое имя от временного сбоя; такие адреса в hosts не подставляются.</p>}
        {summary.needsCheck.length>0&&<details><summary className="cursor-pointer">Какие сервисы требуют проверки</summary><ul className="mt-1 list-inside list-disc break-all">{summary.needsCheck.map(g=><li key={g.name}>{g.name}</li>)}</ul><p>Один сбой не доказывает, что игра не запускается. Повторите замер; если игра работает, принудительная перенастройка не требуется.</p></details>}
        <p className="text-muted-foreground">Это сетевой замер, а не подтверждение входа, матча или голосового чата.{report?.cancelled?" Проверка прервана: результаты неполные.":""}</p>
      </CardContent></Card>}
      {error && <RepairError error={error} />}
      {notice && (
        <p role="status" className="text-xs">
          {notice}
        </p>
      )}
      <details><summary className="cursor-pointer text-xs text-muted-foreground">Ручные настройки и подготовка замера</summary><div className="mt-2 flex flex-col gap-2">
      <Card size="sm">
        <CardHeader>
          <CardTitle>1. Укажите подключение и стратегию</CardTitle>
          <CardDescription>
            Режим указывается вручную. Приложение не переключает VPN и не
            запускает неизвестные BAT-файлы.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <span id="network-region" className="text-xs">
                Регион аккаунта
              </span>
              <Select
                value={region}
                onValueChange={(v) => v && setRegion(v as RunContext["region"])}
                disabled={!!busy}
              >
                <SelectTrigger
                  aria-labelledby="network-region"
                  className="w-full"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {["RU", "EUW", "EUNE", "ALL"].map((r) => (
                      <SelectItem key={r} value={r}>
                        {r === "ALL" ? "Все регионы" : r}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" onClick={choose} disabled={!!busy}>
              <FolderOpen data-icon="inline-start" />
              Выбрать папку Zapret
            </Button>
            <span className="break-all text-xs text-muted-foreground">
              {installation?.root ??
                "Для исходного замера папка не обязательна"}
            </span>
          </div>
          {installation && (
            <div className="flex flex-col gap-2">
              <span id="network-strategy" className="text-xs">
                Точный файл стратегии
              </span>
              <Select
                value={strategy}
                onValueChange={(v) => v && setStrategy(v)}
                disabled={!!busy}
              >
                <SelectTrigger
                  aria-labelledby="network-strategy"
                  className="w-full"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {installation.strategies.map((s) => (
                      <SelectItem key={s} value={s}>
                        {s}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                Запустите именно этот файл самостоятельно. Сохранённое имя — ваш
                выбор, а не подтверждение запущенного процесса.
              </p>
            </div>
          )}
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader>
          <CardTitle>2. Сохраните сетевой замер</CardTitle>
          <CardDescription>
            Для сравнения сделайте исходный замер без VPN и Zapret, затем
            повторите с выбранной стратегией. Каталог: 175 имён, снимок от
            26.10.2026; новые CNAME проверяются автоматически.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <div className="flex flex-wrap items-end gap-2">
            <div className="flex flex-col gap-2">
              <span id="network-mode" className="text-xs">
                Фактический режим подключения
              </span>
              <Select
                value={connection}
                onValueChange={(v) =>
                  v && setConnection(v as RunContext["connection"])
                }
                disabled={!!busy}
              >
                <SelectTrigger aria-labelledby="network-mode">
                  <SelectValue>{connectionLabels[connection]}</SelectValue>
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {Object.entries(connectionLabels).map(([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
            <Button
              onClick={start}
              disabled={
                !!busy ||
                (connection === "zapret" && !strategy)
              }
            >
              {busy === "run" ? (
                <Loader2 className="animate-spin" data-icon="inline-start" />
              ) : (
                <Play data-icon="inline-start" />
              )}
              Начать проверку
            </Button>
            {busy === "run" && (
              <Button
                variant="outline"
                onClick={() => {
                  void cancelNetworkRun().catch((e) => setError(String(e)));
                }}
              >
                <Square data-icon="inline-start" />
                Остановить
              </Button>
            )}
          </div>
          {busy === "run" && progress && (
            <div
              role="status"
              aria-live="polite"
              className="flex flex-col gap-2"
            >
              <span className="text-xs">
                {progress.phase === "dns" ? "DNS" : "Сетевые ответы"}:{" "}
                {progress.completed} / {progress.total || "…"}
              </span>
              <progress
                aria-label="Ход проверки"
                className="w-full"
                max={Math.max(progress.total, 1)}
                value={progress.completed}
              />
              <span className="break-all text-xs text-muted-foreground">
                {progress.current}
              </span>
            </div>
          )}
          <p className="text-xs text-muted-foreground">
            Проверяется весь каталог независимо от региона: DNS системы, Google
            и Cloudflare, IPv4/IPv6, TCP, валидированный TLS, HTTP GET, чат 5223
            и лобби 2099. Зелёный TLS не означает доступ к API. Игровой UDP,
            голос и вход в аккаунт этим прогоном не проверяются.
          </p>
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader>
          <CardTitle>3. Подготовьте изменения</CardTitle>
          <CardDescription>
            Здесь меняется список Riot и параметры выбранного Zapret: это помогает при фильтрации трафика, даже если DNS работает. Основной мастер отдельно исправляет hosts только при подтверждённой ошибке DNS и проверенном рабочем HTTPS. Вручную добавить проверенные адреса можно ниже; DNS адаптера и брандмауэр этот шаг не меняет.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              disabled={!!busy || !installation || !strategy}
              onClick={() =>
                act("preview", async () => {
                  setPatch(
                    await previewNetworkPatch(installation!.root, strategy),
                  );
                })
              }
            >
              Посмотреть изменения
            </Button>
            {backup && (
              <Button
                variant="outline"
                disabled={!!busy}
                onClick={() => setRestoreConfirm(true)}
              >
                <Undo2 data-icon="inline-start" />
                Откатить настройку
              </Button>
            )}
          </div>
          {backup && (
            <p className="break-all text-xs text-muted-foreground">
              Последняя резервная копия: {backup.id} · {backup.root}
            </p>
          )}
          <p className="text-xs text-muted-foreground">
            После применения перезапустите только выбранный Zapret, затем
            выполните новый замер. Приложение не объявляет настройки рабочими до
            сравнения результатов. Если доступа к папке нет, Windows сообщит об
            этом; для системных изменений Windows может запросить права
            администратора.
          </p>
        </CardContent>
      </Card>
      </div></details>
      <Card size="sm">
        <CardHeader>
          <CardTitle>Сохранённые проверки</CardTitle>
          <CardDescription>
            Выберите дату проверки. Сравнение и все отдельные IP доступны при необходимости.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <span id="network-result" className="text-xs">
                Показать замер
              </span>
              <Select
                value={selected || null}
                onValueChange={(v) => v && setSelected(v)}
              >
                <SelectTrigger
                  aria-labelledby="network-result"
                  className="w-full"
                >
                  <SelectValue>{report ? new Date(report.created_at).toLocaleString("ru-RU") : "Замеров пока нет"}</SelectValue>
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {reports.map((r) => (
                      <SelectItem key={r.id} value={r.id}>
                        {new Date(r.created_at).toLocaleString("ru-RU")} ·{" "}
                        {connectionLabels[r.context.connection]} ·{" "}
                        {r.context.strategy ?? "без стратегии"}
                        {r.cancelled ? " · частичный" : ""}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-2">
              <span id="network-before" className="text-xs">
                Сравнить с
              </span>
              <Select
                value={comparison || null}
                onValueChange={(v) => setComparison(v ?? "")}
              >
                <SelectTrigger
                  aria-labelledby="network-before"
                  className="w-full"
                >
                  <SelectValue placeholder="Выберите исходный замер" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {reports
                      .filter((r) => r.id !== selected)
                      .map((r) => (
                        <SelectItem key={r.id} value={r.id}>
                          {new Date(r.created_at).toLocaleString("ru-RU")} ·{" "}
                          {connectionLabels[r.context.connection]}
                        </SelectItem>
                      ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
          </div>
          {diff && (
            <div role="status" className="text-xs">
              Совпадающих проверок: {diff.shared}. TLS восстановлен:{" "}
              {diff.tlsGained}, потерян: {diff.tlsLost}. Отказов Cloudflare:{" "}
              {diff.blocksBefore} → {diff.blocksAfter}.
              {!diff.sameContext && (
                <p className="text-destructive">
                  Провайдер, регион или сетевой профиль отличаются либо не
                  подтверждены: сравнение не доказывает эффективность настройки.
                </p>
              )}
              {(report?.cancelled || before?.cancelled) && (
                <p className="text-muted-foreground">
                  Есть частичный прогон: результаты неполные.
                </p>
              )}
            </div>
          )}
          {report && (
            <>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="outline"
                  size="xs"
                  onClick={() => exportReport(report)}
                >
                  <Download data-icon="inline-start" />
                  Сохранить отчёт для разработчика
                </Button>
              </div>
              <details><summary className="cursor-pointer text-xs">Технические подробности · {report.probes.length} отдельных проб</summary>
              <p className="break-all text-xs text-muted-foreground">
                {report.path}
              </p>
              {report.discovery_limited && (
                <p role="alert" className="text-xs text-destructive">
                  Достигнут предел обхода DNS-цепочек; каталог проверен не
                  полностью.
                </p>
              )}
              {dnsFailures.length > 0 && (
                <details className="text-xs">
                  <summary className="cursor-pointer">
                    Имена без DNS-адресов
                  </summary>
                  <pre className="mt-2 max-h-40 overflow-auto text-xs">
                    {dnsFailures.join("\n")}
                  </pre>
                </details>
              )}
              <Input
                aria-label="Фильтр результатов"
                placeholder="Домен, IP или результат"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
              />
              <div className="max-h-96 overflow-auto">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Домен / IP / порт</TableHead>
                      <TableHead>TCP / TLS</TableHead>
                      <TableHead>Ответ</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {probes.map((p) => (
                      <TableRow key={`${p.domain}|${p.ip}|${p.port}`}>
                        <TableCell className="whitespace-normal">
                          <div>{p.domain}</div>
                          <div className="break-all text-xs text-muted-foreground">
                            {p.request_path}
                          </div>
                          <div className="text-xs text-muted-foreground">
                            {p.ip} : {p.port}
                          </div>
                        </TableCell>
                        <TableCell>
                          {noIpv6Route(p) ? "IPv6 недоступен" : `${p.tcp ? "Да" : "Нет"} / ${p.tls ? "Да" : "Нет"}`}
                        </TableCell>
                        <TableCell className="max-w-sm whitespace-normal">
                          <div>
                            {noIpv6Route(p) ? "Нет маршрута IPv6 с этого ПК" : infrastructureDomain(p.domain) ? "Служебный DNS-узел; прямой HTTPS не обязателен" : classificationLabels[p.classification] ?? p.classification}
                            {p.http ? ` · ${p.http}` : ""}
                          </div>
                          <p className="break-words text-xs text-muted-foreground">
                            {p.title ?? p.error}
                            {p.ray ? ` · Ray: ${p.ray}` : ""}
                          </p>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
              </details>
            </>
          )}
        </CardContent>
      </Card>
      <NetworkHosts report={report} disabled={!!busy} />
      <Dialog
        open={!!patch}
        onOpenChange={(v) => {
          if (!v && !busy) setPatch(null);
        }}
      >
        <DialogContent className="max-h-[85vh] overflow-auto sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle>Изменения {patch?.strategy}</DialogTitle>
            <DialogDescription>
              Сохранится резервная копия BAT и списка доменов. Проверьте
              содержимое до применения. Другая сборка или неподходящий формат
              требуют отдельной настройки.
            </DialogDescription>
          </DialogHeader>
          <details open>
            <summary>Файл после изменения</summary>
            <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 text-xs">
              {patch?.after}
            </pre>
          </details>
          <details>
            <summary>Исходный файл</summary>
            <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 text-xs">
              {patch?.before}
            </pre>
          </details>
          <details>
            <summary>Список доменов</summary>
            <pre className="max-h-48 overflow-auto text-xs">
              {patch?.domains}
            </pre>
          </details>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={!!busy}
              onClick={() => setPatch(null)}
            >
              Отмена
            </Button>
            <Button
              disabled={!!busy}
              onClick={() =>
                act("apply", async () => {
                  const current = patch!;
                  const id = await applyNetworkPatch(current);
                  const next = { root: current.root, id };
                  setBackup(next);
                  localStorage.setItem(
                    "rustlm-network-backup",
                    JSON.stringify(next),
                  );
                  setPatch(null);
                  setNotice(
                    "Изменения записаны. Перезапустите выбранную стратегию и выполните повторный замер.",
                  );
                })
              }
            >
              Применить с резервной копией
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={restoreConfirm} onOpenChange={setRestoreConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Восстановить исходные файлы?</DialogTitle>
            <DialogDescription>
              Остановите выбранный Zapret перед восстановлением. Если файлы были
              изменены после настройки, автоматический откат остановится.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={!!busy}
              onClick={() => setRestoreConfirm(false)}
            >
              Отмена
            </Button>
            <Button
              disabled={!!busy}
              onClick={() =>
                act("restore", async () => {
                  await restoreNetworkPatch(backup!.root, backup!.id);
                  setBackup(null);
                  localStorage.removeItem("rustlm-network-backup");
                  setRestoreConfirm(false);
                  setNotice("Исходные файлы восстановлены.");
                })
              }
            >
              Восстановить
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
