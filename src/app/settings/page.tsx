"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { RefreshCw, Download, Cloud, CloudOff, Trash2, LogIn, LogOut, ShieldCheck, ShieldOff, Info } from "lucide-react";
import { WithTooltip } from "@/components/ui/with-tooltip";

import type { GoodLuckUser, SyncAccountData, SyncStatus } from "@/lib/tauri";
import { goodluckAvatarSrc } from "@/lib/goodluck-display";
import { TotpSetupDialog } from "@/components/totp-setup-dialog";
import { TotpVerifyDialog } from "@/components/totp-verify-dialog";
import { needsCloudTotpVerification } from "@/lib/cloud-totp";
import { toast } from "sonner";

export default function SettingsPage() {
  const [hideLogins, setHideLogins] = useState(false);
  const [autoAcceptMethod, setAutoAcceptMethod] = useState("Polling");
  const [autoUpdate, setAutoUpdate] = useState(true);
  const [minimizeToTray, setMinimizeToTray] = useState(false);
  const [autostart, setAutostart] = useState(false);
  const [autostartBackground, setAutostartBackground] = useState(false);

  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [updateAvailable, setUpdateAvailable] = useState<{ version: string; body?: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  // GoodLuck integration state
  const [glUser, setGlUser] = useState<GoodLuckUser | null>(null);
  const [glSyncedAccounts, setGlSyncedAccounts] = useState<SyncAccountData[]>([]);
  const [glAutoSync, setGlAutoSync] = useState(false);
  const [glSyncing, setGlSyncing] = useState(false);
  const [glLoading, setGlLoading] = useState(false);
  const [glSyncStatus, setGlSyncStatus] = useState<string | null>(null);
  const [glAuthError, setGlAuthError] = useState<string | null>(null);

  // Cloud sync state
  const [cloudSyncStatus, setCloudSyncStatus] = useState<SyncStatus>({ type: "Disconnected" });
  const [cloudSyncing, setCloudSyncing] = useState(false);
  const [cloudDeleting, setCloudDeleting] = useState(false);
  const [totpCloudDialogOpen, setTotpCloudDialogOpen] = useState(false);
  const [needsCloudTotp, setNeedsCloudTotp] = useState(false);
  const pendingCloudAction = useRef<null | (() => Promise<void>)>(null);

  // TOTP 2FA state
  const [totpEnabled, setTotpEnabled] = useState(false);
  const [totpSetupOpen, setTotpSetupOpen] = useState(false);
  const [totpDisableCode, setTotpDisableCode] = useState("");
  const [totpDisabling, setTotpDisabling] = useState(false);

  const [cloudDeleteDialogOpen, setCloudDeleteDialogOpen] = useState(false);
  const [cloudDeleteTotpCode, setCloudDeleteTotpCode] = useState("");
  const [cloudDeleteSubmitting, setCloudDeleteSubmitting] = useState(false);

  const loadCloudState = useCallback(async () => {
    try {
      const { cloudGetStatus } = await import("@/lib/tauri");
      const status = await cloudGetStatus();
      setCloudSyncStatus(status);
    } catch {}
    try {
      const { totpGetStatus, cloudTotpSessionActive } = await import("@/lib/tauri");
      const enabled = await totpGetStatus();
      setTotpEnabled(enabled);
      const sessionOk = await cloudTotpSessionActive();
      setNeedsCloudTotp(enabled && !sessionOk);
    } catch {
      setTotpEnabled(false);
      setNeedsCloudTotp(false);
    }
  }, []);

  const runCloudSyncAfterTotpIfNeeded = useCallback(
    async (run: () => Promise<void>) => {
      if (await needsCloudTotpVerification()) {
        pendingCloudAction.current = run;
        setTotpCloudDialogOpen(true);
        return;
      }
      await run();
    },
    []
  );

  const loadGlState = useCallback(async () => {
    try {
      const { goodluckGetUser, goodluckGetSyncedAccounts, loadSetting } = await import("@/lib/tauri");
      const user = await goodluckGetUser();
      setGlUser(user);
      const autoSync = await loadSetting("GoodLuckAutoSync", false);
      setGlAutoSync(autoSync as boolean);
      if (user) {
        try {
          const synced = await goodluckGetSyncedAccounts();
          setGlSyncedAccounts(synced);
        } catch {
          // Server may not be reachable
        }
      }
    } catch {
      // Not in Tauri
    }
  }, []);

  const loadSettings = useCallback(async () => {
    try {
      const { loadSetting, getAutostartEnabled, getAutostartBackground } = await import("@/lib/tauri");
      const [hide, method, update, tray, autostartVal, bgVal] = await Promise.all([
        loadSetting("HideLogins", false),
        loadSetting("AutoAcceptMethod", "Polling"),
        loadSetting("AutoUpdate", true),
        loadSetting("MinimizeToTray", false),
        getAutostartEnabled(),
        getAutostartBackground(),
      ]);
      setHideLogins(hide as boolean);
      setAutoAcceptMethod(method as string);
      setAutoUpdate(update as boolean);
      setMinimizeToTray(tray as boolean);
      setAutostart(autostartVal);
      setAutostartBackground(bgVal);
    } catch {
      // Not in Tauri
    }
  }, []);

  useEffect(() => {
    loadSettings();
    loadGlState();
    loadCloudState();
  }, [loadSettings, loadGlState, loadCloudState]);

  // Listen for GoodLuck auth events (from deep link)
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
          void loadGlState();
          void loadCloudState();
        });
        const u2 = await listen<string>("goodluck-auth-error", (e) => {
          setGlAuthError(e.payload);
          setGlLoading(false);
        });
        const u3 = await listen<GoodLuckUser>("goodluck-profile-updated", (e) => {
          setGlUser(e.payload);
          loadGlState();
        });
        const u4 = await listen("goodluck-logged-out", () => {
          setGlUser(null);
          setGlSyncedAccounts([]);
          setGlAuthError(null);
          setGlSyncStatus(null);
          setGlLoading(false);
          setTotpCloudDialogOpen(false);
          pendingCloudAction.current = null;
          void loadGlState();
          void loadCloudState();
        });
        const u5 = await listen("cloud-sync-complete", () => {
          void loadSettings();
          void loadCloudState();
        });
        const u6 = await listen("cloud-totp-required", () => {
          void loadCloudState();
        });
        unlisten = () => { u1(); u2(); u3(); u4(); u5(); u6(); };
      } catch { }
    })();
    return () => unlisten?.();
  }, [loadGlState, loadCloudState, loadSettings]);

  const handleGlLogin = async () => {
    setGlAuthError(null);
    setGlLoading(true);
    try {
      const { goodluckLogin } = await import("@/lib/tauri");
      await goodluckLogin();
    } catch {
      setGlLoading(false);
    }
  };

  const handleGlLogout = async () => {
    try {
      const { goodluckLogout } = await import("@/lib/tauri");
      await goodluckLogout();
      setGlUser(null);
      setGlSyncedAccounts([]);
    } catch { }
  };

  const handleGlProfileAndAccountsSync = async () => {
    setGlSyncing(true);
    setGlSyncStatus(null);
    try {
      const { goodluckRefreshProfile, goodluckGetSyncedAccounts, goodluckSyncAccounts } = await import("@/lib/tauri");
      const u = await goodluckRefreshProfile();
      setGlUser(u);
      const result = await goodluckSyncAccounts();
      setGlSyncStatus(
        `Профиль обновлён. Riot в GoodLuck: ${result.created} новых, ${result.updated} обновлено, ${result.skipped} пропущено`
      );
      try {
        const synced = await goodluckGetSyncedAccounts();
        setGlSyncedAccounts(synced);
      } catch { }
      await loadGlState();
    } catch (e) {
      setGlSyncStatus(`Ошибка: ${String(e)}`);
    } finally {
      setGlSyncing(false);
    }
  };

  const save = async (key: string, value: unknown) => {
    try {
      const { saveSetting } = await import("@/lib/tauri");
      await saveSetting(key, value);
    } catch {
      // Not in Tauri
    }
  };

  const handleCheckUpdate = async () => {
    setChecking(true);
    setUpdateStatus(null);
    setUpdateAvailable(null);
    try {
      const { checkForUpdate } = await import("@/lib/tauri");
      const result = await checkForUpdate();
      if (result.available && result.version) {
        setUpdateAvailable({ version: result.version, body: result.body });
      } else {
        setUpdateStatus("Вы используете последнюю версию");
      }
    } catch {
      setUpdateStatus("Не удалось проверить обновления");
    } finally {
      setChecking(false);
    }
  };

  const handleInstallUpdate = async () => {
    setInstalling(true);
    try {
      const { installUpdate } = await import("@/lib/tauri");
      await installUpdate();
    } catch {
      setUpdateStatus("Ошибка установки обновления");
      setInstalling(false);
    }
  };

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Настройки</h1>

      <div className="grid gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Интерфейс</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm">Скрывать логины</span>
              <Switch
                checked={hideLogins}
                onCheckedChange={(v) => {
                  setHideLogins(v);
                  save("HideLogins", v);
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm">Сворачивать в трей</span>
              <Switch
                checked={minimizeToTray}
                onCheckedChange={(v) => {
                  setMinimizeToTray(v);
                  save("MinimizeToTray", v);
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm">Запускать вместе с Windows</span>
              <Switch
                checked={autostart}
                onCheckedChange={async (v) => {
                  setAutostart(v);
                  if (!v) setAutostartBackground(false);
                  try {
                    const { setAutostartEnabled } = await import("@/lib/tauri");
                    await setAutostartEnabled(v);
                  } catch { }
                }}
              />
            </div>
            {autostart && (
              <div className="flex items-center justify-between pl-4">
                <span className="text-sm text-muted-foreground">Запускать в фоне</span>
                <Switch
                  checked={autostartBackground}
                  onCheckedChange={async (v) => {
                    setAutostartBackground(v);
                    try {
                      const { setAutostartBackground } = await import("@/lib/tauri");
                      await setAutostartBackground(v);
                    } catch { }
                  }}
                />
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Авто-принятие</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between">
              <span className="text-sm">Метод</span>
              <Select
                value={autoAcceptMethod}
                onValueChange={(v) => {
                  if (v) {
                    setAutoAcceptMethod(v);
                    save("AutoAcceptMethod", v);
                  }
                }}
              >
                <SelectTrigger className="w-[200px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="WebSocket">WebSocket</SelectItem>
                  <SelectItem value="Polling">Polling</SelectItem>
                  <SelectItem value="UIA">UIA</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base flex items-center gap-2">
              <Cloud className="h-4 w-4" />
              Интеграция GoodLuck
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {glAuthError && (
              <p className="text-sm text-destructive whitespace-pre-wrap wrap-break-words">{glAuthError}</p>
            )}
            {glUser ? (
              <>
                {/* Connected user info */}
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex min-w-0 items-center gap-2">
                    {goodluckAvatarSrc(glUser) ? (
                      <img
                        src={goodluckAvatarSrc(glUser)}
                        alt=""
                        className="h-8 w-8 shrink-0 rounded-full"
                        onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
                      />
                    ) : (
                      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-primary/20">
                        <Cloud className="h-4 w-4 text-primary" />
                      </div>
                    )}
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">{glUser.display_name}</div>
                      <div className="text-xs text-muted-foreground">Подключено</div>
                    </div>
                  </div>
                  <div className="flex shrink-0 flex-wrap items-center justify-end gap-1">
                    <WithTooltip label="Обновить профиль (ник, аватар) и синхронизировать Riot-аккаунты с GoodLuck">
                      <Button
                        variant="outline"
                        size="icon"
                        className="h-9 w-9"
                        onClick={handleGlProfileAndAccountsSync}
                        disabled={glSyncing}
                        aria-label="Синхронизация с GoodLuck"
                      >
                        <RefreshCw className={`h-4 w-4 ${glSyncing ? "animate-spin" : ""}`} />
                      </Button>
                    </WithTooltip>
                    <WithTooltip label="Выйти из GoodLuck">
                      <Button
                        variant="outline"
                        size="icon"
                        className="h-9 w-9"
                        onClick={handleGlLogout}
                        disabled={glSyncing}
                        aria-label="Выйти из GoodLuck"
                      >
                        <LogOut className="h-4 w-4" />
                      </Button>
                    </WithTooltip>
                  </div>
                </div>

                {/* Cloud synced accounts */}
                {glSyncedAccounts.length > 0 && (
                  <div className="rounded-md border border-border p-3 space-y-2">
                    <div className="text-xs text-muted-foreground font-medium">
                      Аккаунты в облаке ({glSyncedAccounts.length})
                    </div>
                    {glSyncedAccounts.map((acc, i) => (
                      <div key={i} className="flex items-center gap-2 text-sm">
                        <Cloud className="h-3 w-3 text-primary/60" />
                        <span className="truncate">{acc.riot_id || acc.summoner_name || "—"}</span>
                        {acc.server && (
                          <span className="text-xs text-muted-foreground">{acc.server}</span>
                        )}
                      </div>
                    ))}
                  </div>
                )}

                {/* Auto-sync toggle */}
                <div className="flex items-center justify-between">
                  <span className="text-sm">Авто-синхронизация</span>
                  <Switch
                    checked={glAutoSync}
                    onCheckedChange={(v) => {
                      setGlAutoSync(v);
                      save("GoodLuckAutoSync", v);
                    }}
                  />
                </div>

                {/* Sync status */}
                {glSyncStatus && (
                  <p className="text-xs text-muted-foreground">{glSyncStatus}</p>
                )}
              </>
            ) : (
              <div className="flex flex-col items-center gap-3 py-2">
                <CloudOff className="h-8 w-8 text-muted-foreground/50" />
                <p className="text-sm text-muted-foreground text-center">
                  Подключите аккаунт GoodLuck для синхронизации Riot-аккаунтов в облаке
                </p>
                <WithTooltip label={glLoading ? "Открываю браузер…" : "Войти через GoodLuck"}>
                  <Button
                    size="icon"
                    className="h-10 w-10"
                    onClick={handleGlLogin}
                    disabled={glLoading}
                    aria-label="Войти через GoodLuck"
                  >
                    <LogIn className={`h-5 w-5 ${glLoading ? "opacity-50" : ""}`} />
                  </Button>
                </WithTooltip>
              </div>
            )}
          </CardContent>
        </Card>

        {glUser && (
          <Card>
            <CardHeader className="flex flex-row items-start justify-between gap-2 space-y-0 pb-4">
              <CardTitle className="text-base flex items-center gap-2">
                <Cloud className="h-4 w-4" />
                Облачное хранилище
              </CardTitle>
              <WithTooltip
                label="Облако синхронизируется автоматически после входа в аккаунт и при изменениях. Ручная синхронизация — кнопками ниже."
                side="left"
              >
                <button
                  type="button"
                  className="text-muted-foreground hover:text-foreground -mt-0.5 shrink-0 rounded-md p-1 outline-offset-2"
                  aria-label="Как работает синхронизация с облаком"
                >
                  <Info className="h-4 w-4" aria-hidden />
                </button>
              </WithTooltip>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="text-sm">
                  Статус:{" "}
                  <span className={
                    cloudSyncStatus.type === "Success" ? "text-green-500" :
                    cloudSyncStatus.type === "Error" ? "text-destructive" :
                    cloudSyncStatus.type === "Syncing" ? "text-yellow-500" :
                    "text-muted-foreground"
                  }>
                    {cloudSyncStatus.type === "Success" ? "Синхронизировано" :
                     cloudSyncStatus.type === "Error" && cloudSyncStatus.message === "totp_required" ? "Нужен код 2FA" :
                     cloudSyncStatus.type === "Error" && cloudSyncStatus.message === "goodluck_reauth" ? "Войдите в GoodLuck снова" :
                     cloudSyncStatus.type === "Error" ? `Ошибка: ${cloudSyncStatus.message}` :
                     cloudSyncStatus.type === "Syncing" ? "Синхронизация..." :
                     cloudSyncStatus.type === "Idle" ? "Готово" :
                     "Не подключено"}
                  </span>
                </div>
              </div>
              {"lastSynced" in cloudSyncStatus && cloudSyncStatus.type === "Success" && (
                <p className="text-xs text-muted-foreground">
                  Последняя синхронизация: {new Date(cloudSyncStatus.lastSynced).toLocaleString()}
                </p>
              )}
              {glUser && needsCloudTotp && (
                <p className="text-xs text-amber-600 dark:text-amber-500">
                  Нужен код из Authenticator для облака (~1 ч). Окно ввода откроется само при истечении сессии; вручную — «Код для облака» или иконка облака в сайдбаре.
                </p>
              )}
              <div className="flex flex-wrap items-center gap-2">
                <WithTooltip label="Полная синхронизация с облаком (отправка и загрузка)">
                  <Button
                    variant="outline"
                    size="icon"
                    className="h-9 w-9"
                    disabled={cloudSyncing}
                    aria-label="Синхронизировать с облаком"
                    onClick={async () => {
                      await runCloudSyncAfterTotpIfNeeded(async () => {
                        setCloudSyncing(true);
                        try {
                          const { cloudSync } = await import("@/lib/tauri");
                          await cloudSync();
                          toast.success("Синхронизация завершена");
                        } catch (e) {
                          toast.error(`Ошибка: ${e}`);
                        } finally {
                          setCloudSyncing(false);
                          loadCloudState();
                        }
                      });
                    }}
                  >
                    <RefreshCw className={`h-4 w-4 ${cloudSyncing ? "animate-spin" : ""}`} />
                  </Button>
                </WithTooltip>
                <WithTooltip label="Загрузить новые аккаунты из облака">
                  <Button
                    variant="outline"
                    size="icon"
                    className="h-9 w-9"
                    aria-label="Загрузить из облака"
                    onClick={async () => {
                      await runCloudSyncAfterTotpIfNeeded(async () => {
                        try {
                          const { cloudPull } = await import("@/lib/tauri");
                          const count = await cloudPull();
                          toast.success(`Загружено из облака: ${count} новых`);
                          loadCloudState();
                        } catch (e) {
                          toast.error(`Ошибка: ${e}`);
                        }
                      });
                    }}
                  >
                    <Download className="h-4 w-4" />
                  </Button>
                </WithTooltip>
                <WithTooltip label="Удалить данные из облака">
                  <Button
                    variant="destructive"
                    size="icon"
                    className="h-9 w-9"
                    disabled={cloudDeleting}
                    aria-label="Удалить данные из облака"
                    onClick={() => {
                      setCloudDeleteTotpCode("");
                      setCloudDeleteDialogOpen(true);
                    }}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </WithTooltip>
              </div>
            </CardContent>
          </Card>
        )}

        {glUser && (
          <Card>
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2">
                <ShieldCheck className="h-4 w-4" />
                Двухфакторная аутентификация
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {totpEnabled ? (
                    <ShieldCheck className="h-4 w-4 text-green-500" />
                  ) : (
                    <ShieldOff className="h-4 w-4 text-muted-foreground" />
                  )}
                  <span className="text-sm">
                    {totpEnabled ? "2FA включена" : "2FA отключена"}
                  </span>
                </div>
                {totpEnabled ? (
                  <div className="flex items-center gap-2">
                    <Button variant="outline" size="sm" onClick={() => setTotpCloudDialogOpen(true)}>
                      Код для облака
                    </Button>
                    <input
                      type="text"
                      placeholder="Код"
                      value={totpDisableCode}
                      onChange={(e) => setTotpDisableCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
                      className="w-20 h-8 px-2 text-center text-sm font-mono rounded-md border border-input bg-transparent"
                    />
                    <Button
                      variant="destructive"
                      size="sm"
                      disabled={totpDisableCode.length !== 6 || totpDisabling}
                      onClick={async () => {
                        setTotpDisabling(true);
                        try {
                          const { totpDisable } = await import("@/lib/tauri");
                          await totpDisable(totpDisableCode);
                          setTotpEnabled(false);
                          setTotpDisableCode("");
                          toast.success("2FA отключена");
                        } catch (e) {
                          toast.error(`Неверный код: ${e}`);
                        } finally {
                          setTotpDisabling(false);
                        }
                      }}
                    >
                      Отключить
                    </Button>
                  </div>
                ) : (
                  <Button size="sm" onClick={() => setTotpSetupOpen(true)}>
                    Настроить 2FA
                  </Button>
                )}
              </div>
              <p className="text-xs text-muted-foreground">
                {totpEnabled
                  ? "Для доступа к облачному хранилищу потребуется код из Google Authenticator"
                  : "Добавьте дополнительный уровень защиты для облачного хранилища"}
              </p>
            </CardContent>
          </Card>
        )}

        <Dialog
          open={cloudDeleteDialogOpen}
          onOpenChange={(open) => {
            setCloudDeleteDialogOpen(open);
            if (!open) setCloudDeleteTotpCode("");
          }}
        >
          <DialogContent className="sm:max-w-md" showCloseButton={true}>
            <DialogHeader>
              <DialogTitle>Удалить данные из облака?</DialogTitle>
              <DialogDescription>
                Все сохранённые в облаке аккаунты будут безвозвратно удалены с сервера. Локальные аккаунты в приложении останутся.
              </DialogDescription>
            </DialogHeader>
            {totpEnabled && (
              <div className="space-y-2">
                <p className="text-sm text-muted-foreground">
                  Введите код из Authenticator для подтверждения.
                </p>
                <Input
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  placeholder="000000"
                  maxLength={6}
                  className="font-mono text-center text-lg tracking-widest"
                  value={cloudDeleteTotpCode}
                  onChange={(e) => setCloudDeleteTotpCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
                />
              </div>
            )}
            <DialogFooter className="gap-2 sm:gap-0">
              <Button
                type="button"
                variant="outline"
                onClick={() => setCloudDeleteDialogOpen(false)}
                disabled={cloudDeleteSubmitting}
              >
                Отмена
              </Button>
              <Button
                type="button"
                variant="destructive"
                disabled={
                  cloudDeleteSubmitting ||
                  (totpEnabled && cloudDeleteTotpCode.length !== 6)
                }
                onClick={async () => {
                  setCloudDeleteSubmitting(true);
                  try {
                    if (totpEnabled) {
                      const { totpValidate } = await import("@/lib/tauri");
                      await totpValidate(cloudDeleteTotpCode);
                    }
                    setCloudDeleting(true);
                    const { cloudDeleteData } = await import("@/lib/tauri");
                    await cloudDeleteData();
                    toast.success("Данные удалены с сервера");
                    setCloudDeleteDialogOpen(false);
                    setCloudDeleteTotpCode("");
                    loadCloudState();
                  } catch (e) {
                    toast.error(`Не удалось удалить: ${e}`);
                  } finally {
                    setCloudDeleting(false);
                    setCloudDeleteSubmitting(false);
                  }
                }}
              >
                {cloudDeleteSubmitting ? "Удаление…" : "Удалить навсегда"}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        <TotpSetupDialog
          open={totpSetupOpen}
          onOpenChange={setTotpSetupOpen}
          onComplete={() => {
            setTotpEnabled(true);
            loadCloudState();
          }}
        />

        <TotpVerifyDialog
          open={totpCloudDialogOpen}
          onOpenChange={(open) => {
            setTotpCloudDialogOpen(open);
            if (!open) {
              pendingCloudAction.current = null;
            }
          }}
          cloudHint
          onVerified={async () => {
            const fn = pendingCloudAction.current;
            pendingCloudAction.current = null;
            await loadCloudState();
            if (fn) {
              await fn();
            }
          }}
        />

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Обновления</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm">Автоматические обновления</span>
              <Switch
                checked={autoUpdate}
                onCheckedChange={(v) => {
                  setAutoUpdate(v);
                  save("AutoUpdate", v);
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm">
                {updateAvailable
                  ? `Доступна версия ${updateAvailable.version}`
                  : updateStatus ?? "Проверить обновления"}
              </span>
              {updateAvailable ? (
                <Button size="sm" onClick={handleInstallUpdate} disabled={installing}>
                  <Download className="h-3.5 w-3.5 mr-1" />
                  {installing ? "Установка..." : "Установить"}
                </Button>
              ) : (
                <Button variant="outline" size="sm" onClick={handleCheckUpdate} disabled={checking}>
                  <RefreshCw className={`h-3.5 w-3.5 mr-1 ${checking ? "animate-spin" : ""}`} />
                  {checking ? "Проверка..." : "Проверить"}
                </Button>
              )}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
