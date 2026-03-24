import { toast } from "sonner";
import type { AccountInfo, AccountRecord } from "@/lib/tauri";

export async function notifyCloudAfterGoodluckSession(): Promise<void> {
  try {
    const { loadSetting, goodluckIsConnected, goodluckSyncAccounts, cloudNotifyChange } =
      await import("@/lib/tauri");
    if (await goodluckIsConnected()) {
      const autoSync = await loadSetting("GoodLuckAutoSync", false);
      if (autoSync) {
        await goodluckSyncAccounts().catch(() => {});
      }
      await cloudNotifyChange().catch(() => {});
    }
  } catch {}
}

export async function pullProfileFromLcuAfterLogin(
  reloadAccounts: () => Promise<void>,
  options?: { retries?: number; toastOnUpdate?: boolean }
): Promise<void> {
  const maxAttempts = options?.retries ?? 25;
  const toastOnUpdate = options?.toastOnUpdate ?? false;

  const run = async (attemptsLeft: number): Promise<void> => {
    const { invalidateLcuCache, refreshAccountProfileFromLcu } = await import("@/lib/tauri");
    try {
      await invalidateLcuCache();
      const r = await refreshAccountProfileFromLcu();
      if (r.updated) {
        await reloadAccounts();
        if (toastOnUpdate) {
          toast.success("Профиль аккаунта обновлён из клиента");
        }
        return;
      }
      if (r.message.includes("уже совпадают")) {
        return;
      }
      if (attemptsLeft > 1) {
        await new Promise((res) => setTimeout(res, 3000));
        return run(attemptsLeft - 1);
      }
    } catch {
      if (attemptsLeft > 1) {
        await new Promise((res) => setTimeout(res, 3000));
        return run(attemptsLeft - 1);
      }
    }
  };

  await run(maxAttempts);
  await notifyCloudAfterGoodluckSession();
}

export function mergeAccountFromLiveInfo(
  account: AccountRecord,
  info: AccountInfo
): AccountRecord {
  const server = info.server?.trim() ?? "";
  const rid = info.riot_id?.trim() ?? "";
  const pid = info.puuid?.trim() ?? "";
  return {
    ...account,
    ...(server ? { Server: server } : {}),
    SummonerName: info.summoner_name || account.SummonerName,
    RiotId: rid || account.RiotId,
    Puuid: pid || account.Puuid,
    Rank: info.rank || account.Rank,
    RankDisplay: info.rank_display || account.RankDisplay,
    AvatarUrl: info.avatar_url || account.AvatarUrl,
  };
}

export function accountRecordChanged(
  before: AccountRecord,
  after: AccountRecord
): boolean {
  return (
    before.Server !== after.Server ||
    before.SummonerName !== after.SummonerName ||
    before.RiotId !== after.RiotId ||
    before.Puuid !== after.Puuid ||
    before.Rank !== after.Rank ||
    before.RankDisplay !== after.RankDisplay ||
    before.AvatarUrl !== after.AvatarUrl
  );
}
