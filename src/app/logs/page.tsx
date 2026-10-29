"use client";

import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { openRepairFolder, exportRepairReport } from "@/lib/repair";
import { logLevel, appendLogLines } from "@/lib/logs";
import { RepairError } from "@/components/repair-error";

const LOG_FILTERS = ["LOGIN", "HTTP", "UI", "PROC", "INFO", "WARN", "ERROR"] as const;

const LOG_COLORS: Record<string, string> = {
  ERROR: "text-red-400",
  WARN: "text-orange-400",
  HTTP: "text-cyan-400",
  LOGIN: "text-green-400",
  UI: "text-purple-400",
  PROC: "text-yellow-400",
  INFO: "text-muted-foreground",
};

export default function LogsPage() {
  const [logLines, setLogLines] = useState<string[]>([]);
  const [error,setError] = useState("");
  const [filters, setFilters] = useState<Record<string, boolean>>(
    Object.fromEntries(LOG_FILTERS.map((f) => [f, true]))
  );

  const cursor = useRef({offset: 0, generation: 0});
  const fetching = useRef(false);
  const fetchLogs = useCallback(async () => {
    if (fetching.current) return;
    fetching.current = true;
    try {
      const { getLogChunk } = await import("@/lib/tauri");
      const chunk = await getLogChunk(cursor.current.offset, cursor.current.generation);
      cursor.current = {offset: chunk.offset, generation: chunk.generation};
      setLogLines((old) => appendLogLines(old, chunk.lines, chunk.reset));
      setError("");
    } catch {
      setError("Не удалось прочитать журнал. Повторите попытку.");
    } finally { fetching.current = false; }
  }, []);

  const [autoRefresh, setAutoRefresh] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  useEffect(() => {
    if (autoRefresh) {
      intervalRef.current = setInterval(() => { if (!document.hidden) void fetchLogs(); }, 3000);
    } else if (intervalRef.current) {
      clearInterval(intervalRef.current);
    }
    return () => { if (intervalRef.current) clearInterval(intervalRef.current); };
  }, [autoRefresh, fetchLogs]);

  const handleClear = async () => {
    try {
      const { clearLogs } = await import("@/lib/tauri");
      await clearLogs();
      setLogLines([]);
      cursor.current = {offset: 0, generation: 0};
    } catch (e) {
      setError(String(e));
    }
  };

  const handleOpenFile = async () => {
    try {
      const { getLogPath } = await import("@/lib/tauri");
      const { open } = await import("@tauri-apps/plugin-shell");
      const path = await getLogPath();
      await open(path);
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleFilter = (filter: string) => {
    setFilters((prev) => ({ ...prev, [filter]: !prev[filter] }));
  };

  const filteredLines = useMemo(() => {
    return logLines.filter((line) => {
      const level = logLevel(line);
      return !level || filters[level] !== false;
    });
  }, [logLines, filters]);

  const getLineColor = (line: string): string => {
    for (const [key, color] of Object.entries(LOG_COLORS)) {
      if (line.includes(`[${key}]`)) return color;
    }
    return "text-muted-foreground";
  };

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-5 gap-y-2">
        <h1 className="text-lg font-semibold">Логи</h1>
        <div className="grid grid-cols-3 gap-2 lg:grid-cols-6">
          <Button variant="secondary" size="sm" onClick={() => {void openRepairFolder().catch((e) => setError(String(e)));}}>Открыть папку</Button>
          <Button variant="secondary" size="sm" onClick={() => {void exportRepairReport().catch((e) => setError(String(e)));}}>Отчёт разработчику</Button>
          <Button variant="secondary" size="sm" onClick={fetchLogs}>
            Обновить
          </Button>
          <Button
            variant={autoRefresh ? "default" : "secondary"}
            size="sm"
            onClick={() => setAutoRefresh(!autoRefresh)}
          >
            {autoRefresh ? "Авто ●" : "Авто"}
          </Button>
          <Button variant="secondary" size="sm" onClick={handleClear}>
            Очистить
          </Button>
          <Button variant="secondary" size="sm" onClick={handleOpenFile}>
            Открыть файл
          </Button>
        </div>
      <div className="col-start-2 flex flex-wrap items-center justify-between gap-x-2 gap-y-2">
        {LOG_FILTERS.map((filter) => (
          <label
            key={filter}
            className="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer"
          >
            <input
              type="checkbox"
              checked={filters[filter]}
              onChange={() => toggleFilter(filter)}
              className="rounded"
            />
            <span className={LOG_COLORS[filter]}>{{LOGIN:"Вход", HTTP:"Запросы", UI:"Интерфейс", PROC:"Процессы", INFO:"Информация", WARN:"Предупреждения", ERROR:"Ошибки"}[filter]}</span>
          </label>
        ))}
      </div>

      </div>
      {error && <RepairError error={error} />}
      <p className="text-xs text-muted-foreground">Последние 2000 строк · полный журнал доступен в файле</p>
      <Card>
        <CardContent className="p-0">
          <ScrollArea className="h-[calc(100vh-240px)]">
            <div className="p-4 font-mono text-xs space-y-0.5">
              {filteredLines.length === 0 ? (
                <p className="text-muted-foreground">Логи пусты</p>
              ) : (
                filteredLines.map((line, i) => (
                  <div key={i} className={getLineColor(line)}>
                    {line}
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}
