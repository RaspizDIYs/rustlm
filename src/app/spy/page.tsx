"use client";

export default function SpyPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Разведка</h1>

      <div className="rounded-xl border border-destructive/50 bg-destructive/10 p-4">
        <p className="text-sm text-destructive">
          Эта функция использует Riot API и находится в серой зоне. Используйте на свой страх и риск.
        </p>
      </div>

      <div className="rounded-xl border border-border bg-card p-6 space-y-4">
        <h2 className="text-lg font-semibold">Настройки API</h2>
        <p className="text-sm text-muted-foreground">Настройте Riot API ключ и регион для работы функции</p>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div className="rounded-xl border border-border bg-card p-6">
          <h2 className="text-lg font-semibold mb-4">Союзники</h2>
          <p className="text-sm text-muted-foreground">Нет данных</p>
        </div>
        <div className="rounded-xl border border-border bg-card p-6">
          <h2 className="text-lg font-semibold mb-4">Противники</h2>
          <p className="text-sm text-muted-foreground">Нет данных</p>
        </div>
      </div>
    </div>
  );
}
