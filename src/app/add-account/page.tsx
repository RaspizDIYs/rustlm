"use client";

export default function AddAccountPage() {
  return (
    <div className="max-w-md space-y-6">
      <h1 className="text-2xl font-bold">Добавить аккаунт</h1>

      <div className="space-y-4 rounded-xl border border-border bg-card p-6">
        <div className="space-y-2">
          <label className="text-sm text-muted-foreground">Логин</label>
          <input
            type="text"
            placeholder="Введите логин"
            className="w-full px-3 py-2 rounded-lg bg-muted border border-border text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary"
          />
        </div>
        <div className="space-y-2">
          <label className="text-sm text-muted-foreground">Пароль</label>
          <input
            type="password"
            placeholder="Введите пароль"
            className="w-full px-3 py-2 rounded-lg bg-muted border border-border text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary"
          />
        </div>
        <div className="space-y-2">
          <label className="text-sm text-muted-foreground">Заметка</label>
          <textarea
            placeholder="Необязательная заметка"
            className="w-full px-3 py-2 rounded-lg bg-muted border border-border text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary resize-none"
            rows={3}
          />
        </div>
        <div className="flex gap-2 pt-2">
          <button className="px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:bg-primary/90 transition-colors">
            Добавить
          </button>
          <button className="px-4 py-2 bg-secondary text-secondary-foreground rounded-lg text-sm hover:bg-accent transition-colors">
            Очистить
          </button>
        </div>
      </div>
    </div>
  );
}
