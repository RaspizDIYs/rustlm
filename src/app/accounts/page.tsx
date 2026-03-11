"use client";

export default function AccountsPage() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Аккаунты</h1>
        <div className="flex gap-2">
          <button className="px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm hover:bg-primary/90 transition-colors">
            Добавить
          </button>
        </div>
      </div>

      <div className="rounded-xl border border-border bg-card p-8 text-center text-muted-foreground">
        <p>Нет добавленных аккаунтов</p>
        <p className="text-sm mt-2">Нажмите «Добавить» чтобы начать</p>
      </div>
    </div>
  );
}
