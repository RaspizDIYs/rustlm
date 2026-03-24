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
      const { isTauri } = await import("@tauri-apps/api/core");
      if (!isTauri()) return;
      const { listen } = await import("@tauri-apps/api/event");

      // When user clicks an account in the tray "Войти" submenu
      unlisten1 = await listen<string>("tray-login", async (event) => {
        const username = event.payload;
        try {
          const { loadAccounts, loginToAccount, invalidateLcuCache } = await import("@/lib/tauri");
          const { pullProfileFromLcuAfterLogin } = await import("@/lib/post-login-sync");
          const accounts = await loadAccounts();
          const account = accounts.find((a) => a.Username === username);
          if (!account) return;

          await loginToAccount(account.Username);
          try {
            await invalidateLcuCache();
          } catch { }
          void pullProfileFromLcuAfterLogin(async () => {
            window.dispatchEvent(new Event("rustlm-accounts-reload"));
          }, {
            retries: 25,
            toastOnUpdate: false,
          }).catch(() => {});
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
