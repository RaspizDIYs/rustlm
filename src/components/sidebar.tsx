"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import {
  Users,
  Zap,
  Palette,
  Search,
  Info,
  Settings,
  ScrollText,
  ChevronLeft,
  ChevronRight,
  Cloud,
  CloudOff,
  CloudUpload,
  RefreshCw,
  LogOut,
  LogIn,
} from "lucide-react";
import {
  goodluckLogin,
  goodluckLogout,
  goodluckGetUser,
  cloudGetStatus,
  cloudSync,
  type GoodLuckUser,
  type SyncStatus,
} from "@/lib/tauri";
import { goodluckAvatarSrc } from "@/lib/goodluck-display";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { TotpVerifyDialog } from "@/components/totp-verify-dialog";
import { needsCloudTotpVerification } from "@/lib/cloud-totp";

const navItems = [
  { href: "/accounts", label: "Аккаунты", icon: Users },
  { href: "/automation", label: "Автоматизация", icon: Zap },
  { href: "/customization", label: "Кастомизация", icon: Palette },
  { href: "/spy", label: "Разведка", icon: Search },
  { href: "/info", label: "Информация", icon: Info },
  { href: "/settings", label: "Настройки", icon: Settings },
  { href: "/logs", label: "Логи", icon: ScrollText },
];

export function Sidebar() {
  const [collapsed, setCollapsed] = useState(false);
  const [appVersion, setAppVersion] = useState("0.1.0");
  const [glUser, setGlUser] = useState<GoodLuckUser | null>(null);
  const [glLoading, setGlLoading] = useState(false);
  const [glAuthError, setGlAuthError] = useState<string | null>(null);
  const [syncStatus, setSyncStatus] = useState<SyncStatus>({ type: "Disconnected" });
  const [syncing, setSyncing] = useState(false);
  const [totpDialogOpen, setTotpDialogOpen] = useState(false);
  const [needsTotpForCloud, setNeedsTotpForCloud] = useState(false);
  const pathname = usePathname();

  const refreshNeedsTotp = useCallback(async () => {
    try {
      setNeedsTotpForCloud(await needsCloudTotpVerification());
    } catch {
      setNeedsTotpForCloud(false);
    }
  }, []);

  const refreshGlUser = useCallback(async () => {
    try {
      const user = await goodluckGetUser();
      setGlUser(user);
    } catch {
      setGlUser(null);
    }
  }, []);

  const refreshSyncStatus = useCallback(async () => {
    try {
      const s = await cloudGetStatus();
      setSyncStatus(s);
    } catch { }
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const { getVersion } = await import("@tauri-apps/api/app");
        setAppVersion(await getVersion());
      } catch { }
    })();
    refreshGlUser();
    refreshSyncStatus();
    void refreshNeedsTotp();
  }, [refreshGlUser, refreshSyncStatus, refreshNeedsTotp]);

  useEffect(() => {
    if (!glUser) {
      setNeedsTotpForCloud(false);
      return;
    }
    void refreshNeedsTotp();
  }, [glUser, refreshNeedsTotp]);

  // Listen for deep link auth events
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { isTauri } = await import("@tauri-apps/api/core");
        if (!isTauri()) return;
        const { listen } = await import("@tauri-apps/api/event");
        const u1 = await listen<GoodLuckUser>("goodluck-auth-success", (e) => {
          setGlAuthError(null);
          setGlUser(e.payload);
          setGlLoading(false);
          void refreshSyncStatus();
          void (async () => {
            await refreshNeedsTotp();
            if (await needsCloudTotpVerification()) {
              setTotpDialogOpen(true);
            }
          })();
        });
        const u2 = await listen<string>("goodluck-auth-error", (e) => {
          setGlAuthError(e.payload);
          setGlLoading(false);
        });
        const u3 = await listen<GoodLuckUser>("goodluck-profile-updated", (e) => {
          setGlUser(e.payload);
        });
        const u4 = await listen("cloud-sync-complete", () => {
          refreshSyncStatus();
          void refreshNeedsTotp();
        });
        const u5 = await listen("cloud-totp-required", () => {
          void refreshSyncStatus();
          void refreshNeedsTotp();
          setTotpDialogOpen(true);
        });
        const u6 = await listen("goodluck-logged-out", () => {
          setGlUser(null);
          setGlAuthError(null);
          setTotpDialogOpen(false);
          void refreshGlUser();
          void refreshSyncStatus();
          void refreshNeedsTotp();
        });
        unlisten = () => { u1(); u2(); u3(); u4(); u5(); u6(); };
      } catch { }
    })();
    return () => unlisten?.();
  }, [refreshGlUser, refreshSyncStatus, refreshNeedsTotp]);

  const handleGlLogin = async () => {
    setGlAuthError(null);
    setGlLoading(true);
    try {
      await goodluckLogin();
    } catch {
      setGlLoading(false);
    }
  };

  const handleGlLogout = async () => {
    try {
      await goodluckLogout();
      setGlUser(null);
    } catch { }
  };

  return (
    <aside
      className={cn(
        "flex h-full shrink-0 flex-col bg-sidebar border-r border-border transition-all duration-300",
        collapsed ? "w-[60px] min-w-[60px] max-w-[60px]" : "w-[220px] min-w-[220px] max-w-[220px]"
      )}
    >
      <div className="flex shrink-0 items-center justify-between border-b border-border p-3">
        {!collapsed && (
          <span className="text-sm font-bold text-foreground tracking-wide">
            RustLM
          </span>
        )}
        <button
          onClick={() => setCollapsed(!collapsed)}
          className="p-1.5 rounded-md hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
        >
          {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
        </button>
      </div>

      <nav className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto p-2">
        {navItems.map((item) => {
          const isActive =
            pathname === item.href || pathname.startsWith(item.href + "/");
          const Icon = item.icon;
          return (
            <Link
              key={item.href}
              href={item.href}
              className={cn(
                "flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-all duration-200",
                isActive
                  ? "bg-sidebar-primary text-sidebar-primary-foreground"
                  : "text-sidebar-foreground hover:bg-accent hover:text-foreground"
              )}
            >
              <Icon className="h-4 w-4 shrink-0" />
              {!collapsed && <span>{item.label}</span>}
            </Link>
          );
        })}
      </nav>

      <div className="shrink-0 space-y-1 border-t border-border p-2">
        {glAuthError && !collapsed && (
          <Tooltip>
            <TooltipTrigger
              render={
                <p className="cursor-default text-[10px] leading-tight text-destructive px-1">
                  {glAuthError.length > 120 ? `${glAuthError.slice(0, 120)}…` : glAuthError}
                </p>
              }
            />
            <TooltipContent side="top" className="max-w-sm whitespace-pre-wrap">
              {glAuthError}
            </TooltipContent>
          </Tooltip>
        )}
        {glUser ? (
          <div className={cn("flex items-center gap-2", collapsed && "justify-center")}>
            <div className="relative shrink-0">
              {goodluckAvatarSrc(glUser) ? (
                <img
                  src={goodluckAvatarSrc(glUser)}
                  alt=""
                  className="h-7 w-7 rounded-full"
                  onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
                />
              ) : (
                <div className="h-7 w-7 rounded-full bg-primary/20 flex items-center justify-center">
                  <Cloud className="h-3.5 w-3.5 text-primary" />
                </div>
              )}
              <span className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full bg-green-500 border-2 border-sidebar" />
            </div>
            {!collapsed && (
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-foreground truncate">
                  {glUser.display_name}
                </div>
                <div className="text-[10px] text-muted-foreground">GoodLuck</div>
              </div>
            )}
            {!collapsed && (
              <div className="flex items-center gap-0.5">
                <Tooltip>
                  <TooltipTrigger
                    render={
                      <button
                        type="button"
                        onClick={async () => {
                          if (syncStatus.type === "Error" && syncStatus.message === "totp_required") {
                            setTotpDialogOpen(true);
                            return;
                          }
                          if (await needsCloudTotpVerification()) {
                            setTotpDialogOpen(true);
                            return;
                          }
                          setSyncing(true);
                          try { await cloudSync(); } catch { } finally {
                            setSyncing(false);
                            refreshSyncStatus();
                            void refreshNeedsTotp();
                          }
                        }}
                        disabled={syncing}
                        className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
                      >
                        {syncing || syncStatus.type === "Syncing" ? (
                          <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                        ) : syncStatus.type === "Error" ? (
                          <CloudOff className="h-3.5 w-3.5 text-destructive" />
                        ) : needsTotpForCloud ? (
                          <Cloud className="h-3.5 w-3.5 text-amber-500" />
                        ) : syncStatus.type === "Success" ? (
                          <Cloud className="h-3.5 w-3.5 text-green-500" />
                        ) : (
                          <CloudUpload className="h-3.5 w-3.5" />
                        )}
                      </button>
                    }
                  />
                  <TooltipContent side="top">
                    {syncStatus.type === "Syncing" ? "Синхронизация..." :
                      syncStatus.type === "Error" && syncStatus.message === "goodluck_reauth" ? "Сессия GoodLuck устарела — войдите снова" :
                        syncStatus.type === "Error" && syncStatus.message === "totp_required" ? "Требуется 2FA — нажмите для ввода кода" :
                          needsTotpForCloud ? "Нужен код 2FA для облака — нажмите" :
                            syncStatus.type === "Success" ? `Синхронизировано` :
                              syncStatus.type === "Error" ? `Ошибка: ${syncStatus.message}` :
                                "Синхронизировать"}
                  </TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger
                    render={
                      <button
                        type="button"
                        onClick={handleGlLogout}
                        className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
                      >
                        <LogOut className="h-3.5 w-3.5" />
                      </button>
                    }
                  />
                  <TooltipContent side="top">Выйти из GoodLuck</TooltipContent>
                </Tooltip>
              </div>
            )}
          </div>
        ) : (
          <Tooltip>
            <TooltipTrigger
              disabled={glLoading}
              render={
                <button
                  type="button"
                  onClick={handleGlLogin}
                  disabled={glLoading}
                  className={cn(
                    "flex items-center gap-2 w-full px-3 py-2 rounded-lg text-sm transition-colors",
                    "text-sidebar-foreground hover:bg-accent hover:text-foreground",
                    "disabled:opacity-50 disabled:cursor-not-allowed",
                    collapsed && "justify-center px-0"
                  )}
                >
                  <LogIn className="h-4 w-4 shrink-0" />
                  {!collapsed && <span>{glLoading ? "Открываю..." : "GoodLuck"}</span>}
                </button>
              }
            />
            <TooltipContent side="right">Войти через GoodLuck</TooltipContent>
          </Tooltip>
        )}
      </div>

      <div className="shrink-0 border-t border-border p-3">
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                className={cn(
                  "text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 rounded",
                  collapsed
                    ? "max-w-full cursor-default truncate text-center text-[10px] leading-none"
                    : "cursor-default text-xs"
                )}
              >
                v{appVersion}
              </button>
            }
          />
          <TooltipContent side="right">Версия {appVersion}</TooltipContent>
        </Tooltip>
      </div>

      <TotpVerifyDialog
        open={totpDialogOpen}
        onOpenChange={setTotpDialogOpen}
        cloudHint
        onVerified={async () => {
          setSyncing(true);
          try { await cloudSync(); } catch { } finally {
            setSyncing(false);
            refreshSyncStatus();
            void refreshNeedsTotp();
          }
        }}
      />
    </aside>
  );
}
