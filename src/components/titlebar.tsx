"use client";

import { useState, useEffect, useCallback } from "react";
import { Minus, Square, X, Maximize2 } from "lucide-react";
import { loadSetting } from "@/lib/tauri";

export function Titlebar() {
  const [maximized, setMaximized] = useState(false);

  const checkMaximized = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      setMaximized(await getCurrentWindow().isMaximized());
    } catch {}
  }, []);

  useEffect(() => {
    checkMaximized();
  }, [checkMaximized]);

  const handleMinimize = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().minimize();
  };

  const handleMaximize = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().toggleMaximize();
    // Small delay to let the window state update
    setTimeout(checkMaximized, 50);
  };

  const handleClose = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const minimizeToTray = await loadSetting<boolean>("MinimizeToTray", false);
    if (minimizeToTray) {
      await getCurrentWindow().hide();
    } else {
      await getCurrentWindow().close();
    }
  };

  const handleDrag = async (e: React.MouseEvent) => {
    // Double-click to maximize/restore
    if (e.detail === 2) {
      await handleMaximize();
      return;
    }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().startDragging();
    } catch {}
  };

  return (
    <div
      className="titlebar flex items-center justify-between h-9 select-none shrink-0"
    >
      {/* Drag region */}
      <div
        className="flex-1 h-full flex items-center px-2.5 gap-2"
        onMouseDown={handleDrag}
      >
        <img src="/icon.png" alt="" className="h-4 w-4" draggable={false} />
      </div>

      {/* Window controls */}
      <div className="flex items-center h-full">
        <button
          onClick={handleMinimize}
          className="titlebar-button h-full w-11 inline-flex items-center justify-center hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
        >
          <Minus className="h-3.5 w-3.5" />
        </button>
        <button
          onClick={handleMaximize}
          className="titlebar-button h-full w-11 inline-flex items-center justify-center hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
        >
          {maximized ? (
            <Square className="h-3 w-3" />
          ) : (
            <Maximize2 className="h-3.5 w-3.5" />
          )}
        </button>
        <button
          onClick={handleClose}
          className="titlebar-button h-full w-11 inline-flex items-center justify-center hover:bg-red-500 hover:text-white text-muted-foreground transition-colors rounded-tr-lg"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
