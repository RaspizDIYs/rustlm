"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Check,
  CircleAlert,
  Download,
  FolderOpen,
  Info,
  Loader2,
  Network,
  Play,
  RotateCcw,
  RefreshCw,
  ShieldCheck,
  Square,
  Wrench,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  applyHosts,
  applyNetworkPatch,
  cancelNetworkRun,
  compareReports,
  inspectZapret,
  previewHosts,
  previewNetworkPatch,
  restoreHosts,
  restoreNetworkPatch,
  runNetwork,
  selectNetworkProfile,
  type Installation,
  type NetworkReport,
  type RunContext,
} from "@/lib/network";
import {
  collectRepairEvidence,
  checkRepairReadiness,
  type RepairReadiness,
  detectZapret,
  restartZapret,
  openRepairFolder,
  exportRepairReport,
  stopZapret,
  openVpn,
  saveRepairJournal,
  snapshotZapret,
  verifyRepairClient,
  type ClientCheck,
  type EvidenceReport,
} from "@/lib/repair";
import {
  dnsRepairPins,
  nextRepairVariant,
  repairSignals,
} from "@/lib/repair-policy";
import {
  cancelLogin,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { RepairError } from "@/components/repair-error";

type Backup = {
  root: string;
  strategy: string;
  id: string;
  changed: boolean;
  hostsId?: string;
  fileRestored?: boolean;
};
type Outcome = { success: boolean; message: string };
const STORAGE = "rustlm-repair-backup";
const steps = [
  "Привычная стратегия",
  "Резервная копия",
  "Проверка и настройка",
  "Защита и откат",
  "Запуск League",
  "Автоподбор и итог",
];
const titles = [
  "Включите привычную стратегию Zapret",
  "Сохраняем исходные настройки",
  "Проверяем и настраиваем подключение",
  "Восстанавливаем настройки",
  "Проверяем запуск League of Legends",
  "Подбираем настройку по результатам",
];

function Notice({
  tone = "info",
  children,
}: {
  tone?: "info" | "warning" | "error" | "success";
  children: React.ReactNode;
}) {
  const Icon =
    tone === "success"
      ? ShieldCheck
      : tone === "error" || tone === "warning"
        ? CircleAlert
        : Info;
  return (
    <div
      role={tone === "error" ? "alert" : "note"}
      className={cn(
        "flex items-start gap-3 rounded-xl border p-4 text-sm leading-relaxed",
        tone === "info" && "border-primary/25 bg-primary/10 text-foreground",
        tone === "warning" && "border-chart-4/30 bg-chart-4/10 text-foreground",
        tone === "error" &&
          "border-destructive/30 bg-destructive/10 text-destructive",
        tone === "success" && "border-chart-2/30 bg-chart-2/10 text-foreground",
      )}
    >
      <Icon aria-hidden className="mt-0.5 size-5 shrink-0" />
      <div className="min-w-0">{children}</div>
    </div>
  );
}
function Ring({ percent }: { percent: number }) {
  const value = Math.max(0, Math.min(100, Math.round(percent)));
  return (
    <div
      role="progressbar"
      aria-label="Ход восстановления"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value}
      className="relative size-32 shrink-0"
    >
      <svg aria-hidden viewBox="0 0 100 100" className="size-full -rotate-90">
        <circle
          cx="50"
          cy="50"
          r="43"
          fill="none"
          stroke="currentColor"
          strokeWidth="6"
          className="text-muted"
        />
        <circle
          cx="50"
          cy="50"
          r="43"
          fill="none"
          stroke="currentColor"
          strokeWidth="6"
          pathLength="100"
          strokeDasharray={`${value} 100`}
          strokeLinecap="round"
          className="text-primary transition-all duration-500"
        />
      </svg>
      <div className="absolute inset-0 flex items-center justify-center text-xl font-semibold tabular-nums">
        {value}%
      </div>
    </div>
  );
}

export default function RepairToolsPage() {
  const [installation, setInstallation] = useState<Installation | null>(null);
  const [strategy, setStrategy] = useState("");
  const provider = "Текущее подключение на этом ПК";
  const region: RunContext["region"] = "ALL";
  const [step, setStep] = useState(1);
  const [busy, setBusy] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [backup, setBackup] = useState<Backup | null>(null);
  const backupRef = useRef<Backup | null>(null);
  const [percent, setPercent] = useState(0);
  const [current, setCurrent] = useState("");
  const [error, setError] = useState("");
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [client, setClient] = useState<ClientCheck | null>(null);
  const [evidence, setEvidence] = useState<EvidenceReport | null>(null);
  const [reports, setReports] = useState<NetworkReport[]>([]);
  const [events, setEvents] = useState<string[]>([]);
  const cancelled = useRef(false);
  const mounted = useRef(true);
  const variantRef = useRef(0);
  const baselineRef = useRef<NetworkReport | null>(null);
  const latestRef = useRef<NetworkReport | null>(null);
  const journalId = useRef<string | null>(null);
  const journalRevision = useRef(0);
  const [journalError, setJournalError] = useState("");
  const [readiness, setReadiness] = useState<RepairReadiness | null>(null);
  const [checkingState, setCheckingState] = useState(false);
  const [stateError, setStateError] = useState("");
  const selectedRoot = installation?.root;
  useEffect(() => {
    if (selectedRoot) localStorage.setItem("rustlm-repair-selection",JSON.stringify({root:selectedRoot,strategy}));
  }, [selectedRoot,strategy]);
  useEffect(() => {
    setReadiness(null);
    setStateError("");
    if (!isTauri() || busy) return;
    let active = true;
    setCheckingState(true);
    void checkRepairReadiness(selectedRoot ?? "", strategy)
      .then((result) => { if (active) setReadiness(result); })
      .catch((cause) => { if (active) setStateError(`Не удалось проверить состояние: ${String(cause)}`); })
      .finally(() => { if (active) setCheckingState(false); });
    return () => { active = false; };
  }, [selectedRoot, strategy, busy]);

  async function refreshReadiness() {
    setCheckingState(true);
    setStateError("");
    try {
      const result = await checkRepairReadiness(installation?.root ?? "", strategy);
      setReadiness(result);
      return result;
    } catch (cause) {
      setStateError(`Не удалось проверить состояние: ${String(cause)}`);
      throw cause;
    } finally { setCheckingState(false); }
  }

  useEffect(() => {
    if (!journalId.current || !isTauri()) return;
    void saveRepairJournal({
      id: journalId.current,
      revision: ++journalRevision.current,
      step,
      provider,
      region,
      strategy,
      events: events.slice(-500),
      report_ids: reports.map((report) => report.id).slice(-20),
      client_status: client?.status ?? null,
      outcome: outcome?.message ?? null,
      operation_failed: Boolean(error),
    }).catch((cause) => setJournalError(`Не удалось сохранить журнал: ${String(cause)}`));
  }, [step, provider, region, strategy, events, reports, client, outcome, error]);

  function persist(value: Backup | null) {
    backupRef.current = value;
    setBackup(value);
    if (value) localStorage.setItem(STORAGE, JSON.stringify(value));
    else localStorage.removeItem(STORAGE);
  }
  function event(message: string) {
    setCurrent(message);
    setEvents((old) => [
      ...old,
      `${new Date().toLocaleTimeString("ru-RU")} · ${message}`,
    ]);
  }
  function checkCancelled() {
    if (cancelled.current)
      throw new Error("Проверка остановлена пользователем");
  }
  const detect = async () => {
    setDetecting(true);
    setError("");
    try {
      const found = await detectZapret();
      if (!mounted.current) return;
      if (found) {
        setInstallation(found.installation);
        setStrategy(found.strategy ?? "");
        event(
          found.strategy
            ? `Найдена стратегия ${found.strategy}`
            : "Zapret найден. Выберите точный файл привычной стратегии.",
        );
      } else
        event(
          "Запущенный Zapret не найден. Включите его и повторите определение или выберите папку.",
        );
    } catch (e) {
      if (mounted.current && !String(e).includes("ZAPRET_DETAILS_UNAVAILABLE")) setError(String(e));
    } finally {
      if (mounted.current) setDetecting(false);
    }
  };
  useEffect(() => {
    mounted.current = true;
    if (!isTauri()) {
      setError("Repair Tools работают в десктопной версии RustLM.");
      return;
    }
    try {
      const saved = localStorage.getItem(STORAGE);
      if (saved) {
        const b = JSON.parse(saved) as Backup;
        if (typeof b.root === "string" && typeof b.id === "string") {
          backupRef.current = b;
          setBackup(b);
        }
      }
    } catch {
      setError(
        "Указатель резервной копии повреждён. Копии остаются в папке данных RustLM → Repair Tools.",
      );
    }
    void (async () => {
      try {
        const saved = JSON.parse(localStorage.getItem("rustlm-repair-selection") ?? "null") ?? backupRef.current;
        if (saved && typeof saved.root === "string") {
          const info = await inspectZapret(saved.root);
          if (!mounted.current) return;
          setInstallation(info);
          setStrategy(info.strategies.includes(saved.strategy) ? saved.strategy : "");
          return;
        }
      } catch (cause) { if (mounted.current) setError(`Не удалось открыть сохранённую папку: ${String(cause)}`); }
      await detect();
    })();
    return () => {
      mounted.current = false;
      cancelled.current = true;
      void cancelNetworkRun().catch(() => {});
    };
    // Start discovery once, independent of wizard form edits.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const choose = async () => {
    try {
      const folder = await open({
        directory: true,
        multiple: false,
        title: "Папка вашего Zapret",
      });
      if (typeof folder !== "string") return;
      const info = await inspectZapret(folder);
      setInstallation(info);
      setStrategy("");
      setError("");
      if (!backupRef.current && info.backups.length)
        persist({
          root: info.root,
          strategy: "",
          id: info.backups[info.backups.length - 1],
          changed: true,
        });
    } catch (e) {
      setError(String(e));
    }
  };

  const rollback = async (restart = true) => {
    let b = backupRef.current;
    if (!b) return;
    setStep(4);
    event("Восстанавливаем резервную копию");
    if (b.hostsId) {
      await restoreHosts(b.hostsId);
      b = { ...b, hostsId: undefined };
      persist(b);
    }
    if (!b.fileRestored) {
      await restoreNetworkPatch(b.root, b.id);
      b = { ...b, fileRestored: true };
      persist(b);
    }
    if (restart && b.changed && b.strategy) {
      event(
        "Перезапускаем Zapret с исходными настройками. Windows может запросить права администратора.",
      );
      await restartZapret(b.root, b.strategy);
    }
    persist(null);
    event("Исходные файлы восстановлены");
  };
  const measure = async (start: number, end: number) => {
    checkCancelled();
    const report = await runNetwork(
      { connection: "zapret", provider, region, strategy, strategy_hash: null },
      installation!.root,
      (progress) => {
        if (!mounted.current) return;
        const fraction = progress.total
          ? progress.completed / progress.total
          : 0;
        const stage =
          progress.phase === "dns" ? fraction * 0.3 : 0.3 + fraction * 0.7;
        setPercent((old) => Math.max(old, start + (end - start) * stage));
        setCurrent(
          `${progress.phase === "dns" ? "DNS" : "Сеть"}: ${progress.completed}/${progress.total} · ${progress.current}`,
        );
      },
    );
    checkCancelled();
    if (
      report.cancelled ||
      report.discovery_limited ||
      report.probes.length === 0
    )
      throw new Error(
        "Сетевой замер неполный. Настройка не будет объявлена успешной.",
      );
    latestRef.current = report;
    setReports((old) => [...old, report]);
    return report;
  };
  const collect = async () => {
    event("Собираем сетевые ошибки Riot, Zapret и Windows");
    const result = await collectRepairEvidence();
    setEvidence(result);
    return result;
  };
  const finishFailure = async (message: string) => {
    await collect().catch((e) => event(`Сбор части журналов недоступен: ${e}`));
    await rollback();
    setPercent(100);
    setStep(6);
    setOutcome({
      success: false,
      message: `${message} Исходные файлы восстановлены.`,
    });
  };
  const checkClient = async () => {
    setStep(5);
    event("Проверяем запуск Riot, авторизацию League и API чата");
    const result = await verifyRepairClient();
    checkCancelled();
    setClient(result);
    return result;
  };
  const configure = async (from: number) => {
    const baseline = baselineRef.current!;
    for (let variant = from; variant <= 2; variant++) {
      variantRef.current = variant;
      checkCancelled();
      setStep(variant === 0 ? 3 : 6);
      event(
        variant === 0
          ? "Добавляем Riot в выбранную стратегию"
          : `Автоподбор ${variant + 1}/3: проверяем другой способ обработки TLS`,
      );
      const plan = await previewNetworkPatch(
        installation!.root,
        strategy,
        variant,
        baseline.id,
      );
      checkCancelled();
      const id = await applyNetworkPatch(plan);
      setInstallation(await inspectZapret(installation!.root));
      persist({
        root: installation!.root,
        strategy,
        id,
        changed: true,
        hostsId: backupRef.current?.hostsId,
      });
      event(
        "Проверяем параметры и перезапускаем только выбранный Zapret. Подтвердите запрос Windows, если он появится.",
      );
      await restartZapret(installation!.root, strategy);
      checkCancelled();
      const after = await measure(30 + variant * 15, 45 + variant * 15);
      const diff = compareReports(baseline, after);
      if (
        baseline.context.network_fingerprint &&
        after.context.network_fingerprint &&
        !diff.sameContext
      )
        throw new Error(
          "Во время проверки изменилось подключение. Повторите мастер в одной сети.",
        );
      event(
        `Сопоставлено ${diff.shared} проверок: восстановлено TLS ${diff.tlsGained}, потеряно ${diff.tlsLost}`,
      );
      const pins = dnsRepairPins(after);
      if (pins.length && !backupRef.current?.hostsId) {
        event(
          `Найдена ошибка системного DNS. Фиксируем ${pins.length} адресов, подтверждённых успешным HTTPS.`,
        );
        const hostsPlan = await previewHosts(after.id, pins);
        checkCancelled();
        const hostsId = await applyHosts(hostsPlan);
        persist({ ...backupRef.current!, hostsId });
        checkCancelled();
      }
      const result = await checkClient();
      if (result.status === "ready") {
        await collect().catch(() => {});
        setStep(6);
        setPercent(100);
        setOutcome({ success: true, message: result.message });
        return;
      }
      if (result.status === "not_installed") {
        await collect().catch(() => {});
        setStep(6);
        setPercent(100);
        setOutcome({
          success: false,
          message:
            "Настройка записана и сетевые замеры сохранены. На этом ПК нет League: работоспособность игры не подтверждена. Можно перенести приложение на игровой ПК и повторить мастер там.",
        });
        return;
      }
      if (result.status === "login_required") {
        event(result.message);
        return;
      }
      await collect();
      const signals = repairSignals(after);
      if (nextRepairVariant(variant, after, result.status) === null) {
        await finishFailure(
          signals.cloudflareBlocks
            ? "Сеть отвечает, но есть отказы Cloudflare. Перебор TLS не подтверждает устранение серверного отказа."
            : "Доступ к клиенту не восстановлен по проверенным вариантам.",
        );
        return;
      }
      // Restore before the next candidate, so changes never stack across attempts.
      await rollback();
      event(
        "Исходные параметры восстановлены; готовим следующую ограниченную попытку",
      );
      const snapshot = await snapshotZapret(installation!.root, strategy);
      persist({
        root: installation!.root,
        strategy,
        id: snapshot,
        changed: false,
      });
    }
  };
  const start = async () => {
    if (!installation || !strategy || busy) return;
    if (
      backupRef.current?.changed &&
      backupRef.current.root !== installation.root
    ) {
      setError(
        "Сначала откатите незавершённую настройку предыдущей папки Zapret.",
      );
      return;
    }
    setBusy(true);
    cancelled.current = false;
    journalId.current = crypto.randomUUID();
    journalRevision.current = 0;
    setJournalError("");
    setError("");
    setOutcome(null);
    setClient(null);
    setReports([]);
    setEvidence(null);
    setEvents([]);
    setPercent(0);
    let snapshotCreated = false;
    try {
      event("Проверяем VPN, запущенный Zapret и соответствие выбранной стратегии");
      const state = await refreshReadiness();
      if (!state) throw new Error("Не удалось проверить состояние подключения");
      if (state.vpn_signals.length) throw new Error(`VPN_DETECTED: ${state.vpn_signals.join("; ")}`);
      if (!state.running || !state.strategy_matches) {
        event("Включаем выбранную стратегию Zapret. Подтвердите запрос Windows.");
        await restartZapret(installation.root,strategy);
        checkCancelled();
        const confirmed=await refreshReadiness();
        if (!confirmed?.running || !confirmed.strategy_matches) throw new Error("ZAPRET_DETAILS_UNAVAILABLE: не удалось подтвердить запуск выбранной стратегии");
      }
      checkCancelled();
      setStep(2);
      event("Создаём резервную копию до любых изменений");
      const id = await snapshotZapret(installation.root, strategy);
      persist({ root: installation.root, strategy, id, changed: false });
      snapshotCreated = true;
      checkCancelled();
      setPercent(3);
      setStep(3);
      event(
        "Измеряем текущее подключение и автоматически обходим DNS-цепочки Riot",
      );
      baselineRef.current = await measure(3, 28);
      await collect();
      checkCancelled();
      await configure(0);
    } catch (e) {
      setError(String(e));
      if (!snapshotCreated) {
        setStep(1);
        return;
      }
      if (
        /UAC_CANCELLED|SERVICE_MODE|VALIDATION_FAILED|RESTART_NOT_STARTED/.test(String(e)) &&
        backupRef.current
      )
        persist({ ...backupRef.current, changed: false });
      try {
        await rollback();
      } catch (restoreError) {
        setError(
          `${e}. Откат не завершён: ${restoreError}. Резервная копия сохранена.`,
        );
      }
      setStep(6);
      setOutcome({
        success: false,
        message:
          "Мастер остановлен. Проверьте сообщение и журнал. Кнопка отката доступна, пока восстановление не завершено.",
      });
    } finally {
      setBusy(false);
    }
  };
  const recheck = async () => {
    setBusy(true);
    cancelled.current = false;
    setError("");
    try {
      const result = await checkClient();
      if (result.status === "ready") {
        setStep(6);
        setPercent(100);
        setOutcome({ success: true, message: result.message });
      } else if (result.status === "failed") {
        const signals = latestRef.current
          ? repairSignals(latestRef.current)
          : null;
        if (signals?.tlsFailures && variantRef.current < 2) {
          await rollback();
          const id = await snapshotZapret(installation!.root, strategy);
          persist({ root: installation!.root, strategy, id, changed: false });
          await configure(variantRef.current + 1);
        } else await finishFailure(result.message);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const exportBundle = () => {
    const blob = new Blob(
      [
        JSON.stringify(
          {
            created_at: new Date().toISOString(),
            provider,
            region,
            strategy,
            outcome,
            client,
            evidence,
            events,
            reports,
          },
          null,
          2,
        ),
      ],
      { type: "application/json" },
    );
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `rustlm-repair-${Date.now()}.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  const activate = async (configured?: boolean, chosenStrategy = strategy) => {
    if (!installation || !chosenStrategy || busy) return;
    setBusy(true);
    setError("");
    let changed = false;
    try {
      const status = await checkRepairReadiness(installation.root,chosenStrategy);
      if (status?.vpn_signals.length) throw new Error(`VPN_DETECTED: ${status.vpn_signals.join("; ")}`);
      if (configured !== undefined) {
        const id = await selectNetworkProfile(installation.root,chosenStrategy,configured);
        persist({root:installation.root,strategy:chosenStrategy,id,changed:true});
        changed = true;
      }
      event("Включаем выбранную стратегию. Подтвердите запрос Windows.");
      await restartZapret(installation.root,chosenStrategy);
      setInstallation(await inspectZapret(installation.root));
      setReadiness(await checkRepairReadiness(installation.root,chosenStrategy));
      event("Стратегия включена. Состояние обновлено.");
    } catch (cause) {
      setError(String(cause));
      if (changed) {
        try {await rollback(!/RESTART_NOT_STARTED|UAC_CANCELLED/.test(String(cause)));}
        catch (restoreError) {setError(`${cause}. Откат не завершён: ${restoreError}`);}
      }
    } finally {setBusy(false);setStep(1);}
  };
  const managedProfile = installation?.profiles.find((p) => p.strategy === strategy);
  const toggleZapret = async () => {
    if (!installation || busy) return;
    if (!readiness?.running) {await activate();return;}
    setBusy(true);
    setError("");
    try {event("Выключаем выбранный Zapret");await stopZapret(installation.root);await refreshReadiness();}
    catch(cause) {setError(String(cause));}
    finally {setBusy(false);}
  };

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-3">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-2">
          <h1 className="flex items-center gap-3 text-xl font-semibold">
            <Wrench aria-hidden className="size-6 text-primary" />
            Repair Tools
          </h1>
          <p className="text-sm text-muted-foreground">
            Мастер восстановления подключения к League of Legends
          </p>
        </div>
        {backup && (
          <Button
            variant="outline"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                await rollback();
                setStep(1);
                setOutcome(null);
              } catch (e) {
                setError(String(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            <RotateCcw data-icon="inline-start" />
            Откатить изменения
          </Button>
        )}
      </header>
      <div className="flex flex-wrap items-center gap-2" aria-label="Состояние подключения">
        <button type="button" title="Включить / выключить выбранный Zapret" disabled={busy || !installation || !strategy || checkingState} onClick={() => {void toggleZapret();}} className={cn("rounded-full border px-2.5 py-1 text-xs disabled:opacity-60", readiness?.running ? "border-green-500/30 bg-green-500/10 text-green-400" : "border-border text-muted-foreground")}>
          ● Zapret: {readiness ? (readiness.running ? "включён" : "выключен") : "проверяем"}
        </button>
        <button type="button" title="Открыть приложение или настройки VPN" onClick={() => {void openVpn().then((message) => event(message)).catch((e) => setError(String(e)));}} className={cn("rounded-full border px-2.5 py-1 text-xs",readiness?.vpn_signals.length ? "border-amber-500/30 bg-amber-500/10 text-amber-400" : "border-border text-muted-foreground")}>
          ● VPN: {readiness ? (readiness.vpn_signals.length ? "обнаружен" : "не обнаружен") : "проверяем"}
        </button>
        <span title={strategy || "Выберите BAT-файл"} className="rounded-full border px-2.5 py-1 text-xs text-muted-foreground">
          Стратегия: {!strategy ? "не выбрана" : readiness?.details_unavailable ? "нужны права" : readiness?.strategy_matches ? "совпадает" : "не запущена / другая"}
        </span>
        <Button size="icon-sm" variant="ghost" aria-label="Обновить состояние" title="Обновить состояние" disabled={busy || checkingState} onClick={() => {void refreshReadiness().catch(() => {});}}><RefreshCw className={checkingState ? "animate-spin" : ""} /></Button>
        <Button size="sm" disabled={busy || !strategy || !installation || checkingState || Boolean(readiness?.vpn_signals.length)} onClick={() => {void activate();}}><Play />Включить</Button>
        <Button size="icon-sm" variant="ghost" aria-label="Открыть папку Repair Tools" title="Открыть папку Repair Tools" onClick={() => {void openRepairFolder().catch((e) => setError(String(e)));}}><FolderOpen /></Button>
        <Button size="icon-sm" variant="ghost" aria-label="Сохранить отчёт для разработчика" title="Сохранить отчёт для разработчика" disabled={busy} onClick={() => {void exportRepairReport().catch((e) => setError(String(e)));}}><Download /></Button>
      </div>
      <ol
        aria-label="Шаги восстановления"
        className="grid grid-cols-6 gap-1"
      >
        {steps.map((label, index) => (
          <li
            key={label}
            aria-current={step === index + 1 ? "step" : undefined}
            className={cn(
              "flex items-center gap-1.5 rounded-md border px-2 py-1.5 text-[11px]",
              step === index + 1
                ? "border-primary/40 bg-primary/10"
                : "border-border bg-card",
            )}
          >
            <span className="font-semibold text-muted-foreground">
              {index + 1}
            </span>
            <span className="truncate" title={label}>{label}</span>
          </li>
        ))}
      </ol>
      {(error || stateError) && <RepairError error={error || stateError} />}
      {journalError && <Notice tone="warning">{journalError}</Notice>}
      <Card
        size="sm"
        key={step}
        className="animate-in fade-in slide-in-from-right-2 duration-300 motion-reduce:animate-none"
      >
        <CardHeader>
          <CardTitle>
            {outcome
              ? outcome.success
                ? "Проверка клиента пройдена"
                : "Восстановление требует внимания"
              : titles[step - 1]}
          </CardTitle>
          <CardDescription>
            Шаг {step} из 6
            {step === 6 && !outcome
              ? ` · попытка ${variantRef.current + 1} из 3`
              : ""}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {step === 1 && (
            <>
              <p className="text-xs text-muted-foreground">Выберите папку и стратегию — LM включит её сам. VPN можно открыть по чипу сверху.</p>
              <div className="flex flex-wrap gap-3">
                <Button
                  variant="outline"
                  onClick={detect}
                  disabled={busy || detecting}
                >
                  {detecting ? (
                    <Loader2
                      className="animate-spin"
                      data-icon="inline-start"
                    />
                  ) : (
                    <Network data-icon="inline-start" />
                  )}
                  Определить Zapret
                </Button>
                <Button
                  variant="outline"
                  disabled={busy || detecting}
                  onClick={choose}
                >
                  <FolderOpen data-icon="inline-start" />
                  Выбрать папку
                </Button>
              </div>
              {installation && (
                <>
                  <p className="break-all text-xs text-muted-foreground">
                    {installation.root}
                  </p>
                  <div className="flex flex-col gap-2">
                    <span id="repair-strategy" className="text-sm font-medium">
                      Стратегия Zapret
                    </span>
                    <Select
                      value={strategy || null}
                      onValueChange={(s) => { if (s) {setStrategy(s);void activate(undefined,s);} }}
                    >
                      <SelectTrigger
                        aria-labelledby="repair-strategy"
                        className="w-full"
                      >
                        <SelectValue placeholder="Выберите точное имя BAT-файла" />
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
                  </div>
                </>
              )}
              {managedProfile && <div className="flex flex-wrap items-center gap-2 rounded-lg border p-2">
                <span className="mr-auto text-xs text-muted-foreground">Версия файла: {managedProfile.state === "configured" ? "настроена для Riot" : managedProfile.state === "original" ? "исходная" : "изменена вручную"}</span>
                <Button size="sm" variant="outline" disabled={busy || managedProfile.state === "modified" || Boolean(readiness?.vpn_signals.length)} onClick={() => {void activate(false);}}>Исходная</Button>
                <Button size="sm" variant="outline" disabled={busy || managedProfile.state === "modified" || Boolean(readiness?.vpn_signals.length)} onClick={() => {void activate(true);}}>С настройкой Riot</Button>
              </div>}
              <details className="text-xs text-muted-foreground">
                <summary className="cursor-pointer">Что изменится и где сохраняются копии</summary>
                <p className="mt-2">Меняется выбранная стратегия и её списки Riot; hosts — только при подтверждённой ошибке DNS. Копии и логи: RustLM → Repair Tools. Службы и Zapret2 пока не поддерживаются. Windows может запросить права для включения стратегии.</p>
              </details>
              <Button
                size="sm"
                disabled={
                  !installation ||
                  !strategy ||
                  busy ||
                  detecting || checkingState
                }
                onClick={start}
              >
                <Wrench data-icon="inline-start" />
                Проверить и настроить автоматически
              </Button>
            </>
          )}
          {busy && (
            <div className="flex flex-col items-center gap-5 sm:flex-row">
              <Ring percent={percent} />
              <div className="flex min-w-0 flex-1 flex-col gap-3">
                <p
                  role="status"
                  aria-live="polite"
                  className="break-all text-sm"
                >
                  {current}
                </p>
                <Notice>
                  {step === 2
                    ? "Резервная копия создаётся до изменения настроек. Кнопка отката останется сверху."
                    : step === 5
                      ? "Запускаем существующий клиент и проверяем авторизацию League и ответ API чата. Одного появления процесса недостаточно."
                      : step === 6
                        ? "Попыток не больше трёх. Следующий вариант применяется только при признаках ошибки TLS. Параметры не накладываются друг на друга."
                        : "Проверяем DNS, IPv4/IPv6, TCP, TLS и HTTPS. Ошибка отдельного узла не равна поломке всей игры; отказы Cloudflare отмечаются отдельно."}
                </Notice>
                <Button
                  variant="outline"
                  onClick={() => {
                    cancelled.current = true;
                    event("Останавливаем текущую проверку и готовим откат");
                    void cancelNetworkRun().catch(() => {});
                    void cancelLogin().catch(() => {});
                  }}
                >
                  <Square data-icon="inline-start" />
                  Остановить и откатить
                </Button>
              </div>
            </div>
          )}
          {!busy && step === 5 && client?.status === "login_required" && (
            <>
              <Notice tone="warning">{client.message}</Notice>
              <Button onClick={recheck}>
                <Play data-icon="inline-start" />Я вошёл — проверить снова
              </Button>
            </>
          )}
          {outcome && (
            <>
              <Notice tone={outcome.success ? "success" : "warning"}>
                {outcome.message}
              </Notice>
              <Notice>
                Результат относится к этому ПК, текущему провайдеру и выбранной
                стратегии. Игровой матч, UDP и голос не подтверждаются проверкой
                HTTPS или запуском клиента.
              </Notice>
              <div className="flex flex-wrap gap-3">
                <Button variant="outline" onClick={exportBundle}>
                  <Download data-icon="inline-start" />
                  Сохранить отчёт
                </Button>
                <Button
                  variant="outline"
                  onClick={() => {
                    setStep(1);
                    setOutcome(null);
                    setError("");
                  }}
                >
                  Вернуться к началу
                </Button>
              </div>
            </>
          )}
          {backup && (
            <p className="break-all text-xs text-muted-foreground">
              Копия: {backup.id} · RustLM / Repair Tools
            </p>
          )}
        </CardContent>
      </Card>
      {reports.length > 0 && (
        <div className="flex flex-wrap gap-2">
          <Badge variant="secondary">Сетевых замеров: {reports.length}</Badge>
          <Badge variant="secondary">
            Проверок в последнем: {reports[reports.length - 1].probes.length}
          </Badge>
          <Badge variant="outline">
            Cloudflare:{" "}
            {repairSignals(reports[reports.length - 1]).cloudflareBlocks}
          </Badge>
        </div>
      )}
      {evidence && (
        <details className="rounded-xl border border-border bg-card p-4">
          <summary className="cursor-pointer text-sm font-medium">
            Автоматически собранные ошибки
          </summary>
          <div className="mt-3 flex flex-col gap-2">
            {evidence.errors.map((item, i) => (
              <div key={i} className="flex flex-wrap gap-2 text-sm">
                <Badge variant="outline">{item.category}</Badge>
                <span>
                  {item.source} · {item.count}
                </span>
              </div>
            ))}
            {evidence.errors.length === 0 && (
              <p className="text-sm text-muted-foreground">
                В доступных журналах подходящие ошибки не найдены.
              </p>
            )}
            {evidence.unavailable.map((message, i) => (
              <p key={i} className="text-xs text-muted-foreground">
                {message}
              </p>
            ))}
            <p className="break-all text-xs text-muted-foreground">
              {evidence.path}
            </p>
          </div>
        </details>
      )}
      {events.length > 0 && (
        <details className="rounded-xl border border-border bg-card p-4">
          <summary className="cursor-pointer text-sm font-medium">
            Журнал шагов
          </summary>
          <ol className="mt-3 flex max-h-64 flex-col gap-2 overflow-auto text-xs text-muted-foreground">
            {events.map((item, i) => (
              <li key={i}>{item}</li>
            ))}
          </ol>
        </details>
      )}
      {!busy && (
        <Link
          href="/connection"
          className="text-sm text-muted-foreground underline underline-offset-4"
        >
          Подробная диагностика и ручная настройка
        </Link>
      )}
    </div>
  );
}
