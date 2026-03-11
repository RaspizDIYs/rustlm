"use client";

export default function AutomationPage() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Автоматизация</h1>
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">Включить</span>
          <button className="w-10 h-5 rounded-full bg-muted relative transition-colors">
            <span className="absolute left-0.5 top-0.5 w-4 h-4 rounded-full bg-foreground transition-transform" />
          </button>
        </div>
      </div>

      <div className="grid gap-4">
        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">Выбор чемпиона</h2>
          <p className="text-sm text-muted-foreground">Настройте автоматический выбор чемпиона, бана и заклинаний</p>
        </div>

        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">Страницы рун</h2>
          <p className="text-sm text-muted-foreground">Управление страницами рун для автоматической установки</p>
        </div>
      </div>
    </div>
  );
}
