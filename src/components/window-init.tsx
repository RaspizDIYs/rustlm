"use client";
import { useEffect } from "react";

export function WindowInit() {
  useEffect(() => {
    import("@/lib/tauri").then(({ reportWebviewReady }) => {
      reportWebviewReady().catch(() => {});
    }).catch(() => {});
    import("@/lib/tauri").then(({ shouldStartMinimized, loadSetting }) => {
      loadSetting("AnimationsEnabled", true).then((enabled) => {
        document.documentElement.dataset.animations = enabled ? "on" : "off";
      }).catch(() => {});
      shouldStartMinimized().then((minimized) => {
        if (!minimized) {
          import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
            getCurrentWindow().show().catch(() => {});
          }).catch(() => {});
        }
      }).catch(() => {
        import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
          getCurrentWindow().show().catch(() => {});
        }).catch(() => {});
      });
    }).catch(() => {});
  }, []);
  return null;
}
