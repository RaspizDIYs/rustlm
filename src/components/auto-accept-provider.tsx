"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

type AutoAcceptContextValue = {
  enabled: boolean;
  ready: boolean;
  pending: boolean;
  setAutoAccept: (value: boolean) => Promise<void>;
};

const AutoAcceptContext = createContext<AutoAcceptContextValue | null>(null);

export function AutoAcceptProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [enabled, setEnabled] = useState(false);
  const [ready, setReady] = useState(false);
  const [pending, setPending] = useState(false);
  const enabledRef = useRef(false);
  const togglingRef = useRef(false);

  useEffect(() => {
    enabledRef.current = enabled;
  }, [enabled]);

  useEffect(() => {
    let cancelled = false;
    import("@/lib/tauri")
      .then(({ isAutoAcceptEnabled }) => isAutoAcceptEnabled())
      .then((v) => {
        if (!cancelled) {
          setEnabled(v);
          enabledRef.current = v;
        }
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setReady(true);
      });

    const onSync = (e: Event) => {
      const v = (e as CustomEvent<boolean>).detail;
      setEnabled(v);
      enabledRef.current = v;
    };
    window.addEventListener("autoAcceptSync", onSync);
    return () => {
      cancelled = true;
      window.removeEventListener("autoAcceptSync", onSync);
    };
  }, []);

  const setAutoAccept = useCallback(async (value: boolean) => {
    if (!ready || togglingRef.current) return;
    if (value === enabledRef.current) return;
    togglingRef.current = true;
    setPending(true);
    const prev = enabledRef.current;
    setEnabled(value);
    enabledRef.current = value;
    try {
      const {
        setAutoAcceptEnabled: setCmd,
        refreshTray,
        isAutoAcceptEnabled,
      } = await import("@/lib/tauri");
      await setCmd(value);
      await refreshTray().catch(() => {});
      const confirmed = await isAutoAcceptEnabled();
      setEnabled(confirmed);
      enabledRef.current = confirmed;
    } catch {
      setEnabled(prev);
      enabledRef.current = prev;
    } finally {
      togglingRef.current = false;
      setPending(false);
    }
  }, [ready]);

  const value: AutoAcceptContextValue = {
    enabled,
    ready,
    pending,
    setAutoAccept,
  };

  return (
    <AutoAcceptContext.Provider value={value}>
      {children}
    </AutoAcceptContext.Provider>
  );
}

export function useAutoAccept(): AutoAcceptContextValue {
  const ctx = useContext(AutoAcceptContext);
  if (!ctx) {
    throw new Error("useAutoAccept must be used within AutoAcceptProvider");
  }
  return ctx;
}

export function AutoAcceptSwitch({ className }: { className?: string }) {
  const { enabled, ready, pending, setAutoAccept } = useAutoAccept();
  return (
    <div className={cn("flex items-center gap-2", className)}>
      <span className="text-sm text-muted-foreground">Автопринятие</span>
      <Switch
        checked={enabled}
        disabled={!ready || pending}
        onCheckedChange={(v) => void setAutoAccept(v)}
      />
    </div>
  );
}
