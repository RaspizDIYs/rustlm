"use client";

import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";

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
  const [filters, setFilters] = useState<Record<string, boolean>>(
    Object.fromEntries(LOG_FILTERS.map((f) => [f, true]))
  );

  const fetchLogs = useCallback(async () => {
    try {
      const { getLogLines } = await import("@/lib/tauri");
      const lines = await getLogLines();
      setLogLines(lines);
    } catch {
      // Not in Tauri
    }
  }, []);

  const [autoRefresh, setAutoRefresh] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  useEffect(() => {
    if (autoRefresh) {
      intervalRef.current = setInterval(fetchLogs, 3000);
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
    } catch (e) {
      console.error(e);
    }
  };

  const handleOpenFile = async () => {
    try {
      const { getLogPath } = await import("@/lib/tauri");
      const { open } = await import("@tauri-apps/plugin-shell");
      const path = await getLogPath();
      await open(path);
    } catch (e) {
      console.error(e);
    }
  };

  const toggleFilter = (filter: string) => {
    setFilters((prev) => ({ ...prev, [filter]: !prev[filter] }));
  };

  const filteredLines = useMemo(() => {
    return logLines.filter((line) => {
      const match = line.match(/\[(\w+)\]/g);
      if (!match || match.length < 2) return true;
      const level = match[1].replace(/[\[\]]/g, "");
      return filters[level] !== false;
    });
  }, [logLines, filters]);

  const getLineColor = (line: string): string => {
    for (const [key, color] of Object.entries(LOG_COLORS)) {
      if (line.includes(`[${key}]`)) return color;
    }
    return "text-muted-foreground";
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Логи</h1>
        <div className="flex gap-2">
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
      </div>

      <div className="flex gap-3 flex-wrap">
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
            <span className={LOG_COLORS[filter]}>{filter}</span>
          </label>
        ))}
      </div>

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
