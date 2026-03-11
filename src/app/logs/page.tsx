"use client";

export default function LogsPage() {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Логи</h1>
        <div className="flex gap-2">
          <button className="px-3 py-1.5 bg-secondary text-secondary-foreground rounded-lg text-xs hover:bg-accent transition-colors">
            Обновить
          </button>
          <button className="px-3 py-1.5 bg-secondary text-secondary-foreground rounded-lg text-xs hover:bg-accent transition-colors">
            Очистить
          </button>
          <button className="px-3 py-1.5 bg-secondary text-secondary-foreground rounded-lg text-xs hover:bg-accent transition-colors">
            Открыть файл
          </button>
        </div>
      </div>

      <div className="flex gap-2 flex-wrap">
        {["LOGIN", "HTTP", "UI", "PROC", "INFO", "WARN", "ERROR"].map((filter) => (
          <label key={filter} className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <input type="checkbox" defaultChecked className="rounded" />
            {filter}
          </label>
        ))}
      </div>

      <div className="rounded-xl border border-border bg-card p-4 min-h-[400px] font-mono text-xs text-muted-foreground">
        <p>Логи пусты</p>
      </div>
    </div>
  );
}
