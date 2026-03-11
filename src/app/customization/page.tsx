"use client";

export default function CustomizationPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Кастомизация</h1>

      <div className="grid gap-4">
        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">Статус профиля</h2>
          <p className="text-sm text-muted-foreground">Установите кастомный статус профиля</p>
        </div>

        <div className="rounded-xl border border-border bg-card p-6 space-y-4">
          <h2 className="text-lg font-semibold">Фон профиля</h2>
          <p className="text-sm text-muted-foreground">Выберите скин для фона профиля</p>
        </div>
      </div>
    </div>
  );
}
