"use client";

export default function SettingsPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Настройки</h1>

      <div className="grid gap-4">
        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">Интерфейс</h2>
          <div className="flex items-center justify-between">
            <span className="text-sm">Скрывать логины</span>
            <button className="w-10 h-5 rounded-full bg-muted relative transition-colors">
              <span className="absolute left-0.5 top-0.5 w-4 h-4 rounded-full bg-foreground transition-transform" />
            </button>
          </div>
        </div>

        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">Авто-принятие</h2>
          <p className="text-sm text-muted-foreground">Выберите метод авто-принятия матча</p>
        </div>

        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">Обновления</h2>
          <p className="text-sm text-muted-foreground">Настройки автоматических обновлений</p>
        </div>

        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">League Client</h2>
          <p className="text-sm text-muted-foreground">Путь к установке League of Legends</p>
        </div>
      </div>
    </div>
  );
}
