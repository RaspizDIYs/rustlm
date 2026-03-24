"use client";
import { useEffect } from "react";

export function WindowInit() {
  useEffect(() => {
    import("@/lib/tauri").then(({ shouldStartMinimized }) => {
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
