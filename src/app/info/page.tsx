"use client";

export default function InfoPage() {
  return (
    <div className="space-y-6">
      <div className="flex flex-col items-center justify-center py-12 space-y-4">
        <h1 className="text-4xl font-bold bg-gradient-to-r from-primary to-purple-500 bg-clip-text text-transparent">
          RustLM
        </h1>
        <p className="text-muted-foreground">League of Legends Account Manager</p>
        <p className="text-sm text-muted-foreground">Версия 0.1.0</p>
      </div>

      <div className="rounded-xl border border-border bg-card p-6 space-y-4">
        <h2 className="text-lg font-semibold">Разработчики</h2>
        <div className="space-y-2 text-sm text-muted-foreground">
          <p>@mejaikin</p>
          <p>@spellq</p>
          <p>@shp1n4t</p>
        </div>
      </div>

      <div className="flex gap-2">
        <button className="px-4 py-2 bg-secondary text-secondary-foreground rounded-lg text-sm hover:bg-accent transition-colors">
          Сообщить об ошибке
        </button>
        <button className="px-4 py-2 bg-secondary text-secondary-foreground rounded-lg text-sm hover:bg-accent transition-colors">
          GitHub
        </button>
        <button className="px-4 py-2 bg-secondary text-secondary-foreground rounded-lg text-sm hover:bg-accent transition-colors">
          Discord
        </button>
      </div>
    </div>
  );
}
