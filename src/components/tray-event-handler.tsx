"use client";

import { useEffect } from "react";

/**
 * Global component that listens for tray menu events and triggers
 * appropriate frontend actions (login, auto-accept sync).
 */
export function TrayEventHandler() {
  useEffect(() => {
    let unlisten1: (() => void) | undefined;
    let unlisten2: (() => void) | undefined;

    (async () => {
      const { listen } = await import("@tauri-apps/api/event");

      // When user clicks an account in the tray "Войти" submenu
      unlisten1 = await listen<string>("tray-login", async (event) => {
        const username = event.payload;
        try {
          const { loadAccounts, loginToAccount, detectServer, getAccountInfo, saveAccount } =
            await import("@/lib/tauri");
          const accounts = await loadAccounts();
          const account = accounts.find((a) => a.Username === username);
          if (!account) return;

          await loginToAccount(account.Username, account.EncryptedPassword);
          // Fetch account info & server after login (with retries)
          const fetchInfo = async (retries: number): Promise<void> => {
            let updated = { ...account };
            let changed = false;
            try {
              const server = await detectServer();
              if (server && server !== updated.Server) {
                updated = { ...updated, Server: server };
                changed = true;
              }
            } catch { /* non-critical */ }
            try {
              const info = await getAccountInfo();
              if (info) {
                updated = {
                  ...updated,
                  SummonerName: info.summoner_name || updated.SummonerName,
                  RiotId: info.riot_id || updated.RiotId,
                  Rank: info.rank || updated.Rank,
                  RankDisplay: info.rank_display || updated.RankDisplay,
                  AvatarUrl: info.avatar_url || updated.AvatarUrl,
                };
                changed = true;
              } else if (retries > 0) {
                await new Promise((r) => setTimeout(r, 3000));
                return fetchInfo(retries - 1);
              }
            } catch {
              if (retries > 0) {
                await new Promise((r) => setTimeout(r, 3000));
                return fetchInfo(retries - 1);
              }
            }
            if (changed) {
              await saveAccount(updated);
            }
          };
          fetchInfo(3).catch(() => {});
        } catch {
          // Login errors handled silently from tray
        }
      });

      // When auto-accept is toggled from the tray checkbox
      unlisten2 = await listen<boolean>("auto-accept-changed", async (event) => {
        // Dispatch a custom DOM event so pages can react
        window.dispatchEvent(
          new CustomEvent("autoAcceptSync", { detail: event.payload })
        );
      });
    })();

    return () => {
      unlisten1?.();
      unlisten2?.();
    };
  }, []);

  return null;
}
