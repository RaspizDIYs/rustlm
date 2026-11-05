import type { PartyLocal } from "@/lib/party";

export function partyConnected(local: Pick<PartyLocal, "summonerId" | "riotId" | "error"> | undefined) {
  return !!local?.summonerId && !!local.riotId && !local.error;
}

export function partyActionError(error: unknown) {
  const details = error && typeof error === "object" ? error as { status?: unknown; statusCode?: unknown; message?: unknown } : null;
  const text = error instanceof Error ? error.message : details
    ? `${details.status ?? details.statusCode ?? ""} ${details.message ?? String(error)}`
    : String(error);
  if (/\b401\b/.test(text)) return "Подтвердите свою учётную запись LM заново.";
  if (/\b503\b/.test(text)) return "Сервис пати временно недоступен — попробуйте позже.";
  if (/\b403\b/.test(text)) return "У вас нет прав на это действие в группе.";
  if (/\b409\b|revision|conflict|устар|конфликт/i.test(text)) return "Группа изменилась — проверьте её состояние и повторите действие.";
  if (/(?:league|lcu).*(?:not connected|disconnected|offline|not running)|(?:откройте|запустите).*league/i.test(text)) return "Откройте League и войдите в игровой аккаунт.";
  if (/network|fetch|timeout|connection/i.test(text)) return "Не удалось связаться с сервисом пати — проверьте подключение.";
  return "Не удалось выполнить действие — проверьте состояние группы перед повтором.";
}

// Keep sent mutations locked across visibility changes; queued mutations belong to their epoch.
export function createPartyRequestGate() {
  let mutation = false;
  let reading: Promise<void> | null = null;
  let epoch = 0;
  return {
    invalidate() { epoch++; },
    async read(work: (current: () => boolean) => Promise<void>) {
      if (reading || mutation) return false;
      let release!: () => void;
      reading = new Promise<void>(resolve => { release = resolve; });
      const started = epoch;
      try { await work(() => started === epoch); }
      finally { reading = null; release(); }
      return true;
    },
    async run(work: (current: () => boolean) => Promise<void>, pending: () => void = () => {}) {
      if (mutation) return false;
      mutation = true;
      const started = epoch;
      const current = () => started === epoch;
      try {
        pending();
        if (reading) await reading;
        if (!current()) return false;
        await work(current);
        return true;
      } finally { mutation = false; }
    },
  };
}
