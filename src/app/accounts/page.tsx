"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Download, Upload, Loader2, Lock } from "lucide-react";
import type { AccountRecord, ClientConnectivityStatus } from "@/lib/tauri";

export default function AccountsPage() {
  const [accounts, setAccounts] = useState<AccountRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [loginInProgress, setLoginInProgress] = useState<string | null>(null);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [connectivity, setConnectivity] = useState<ClientConnectivityStatus | null>(null);
  const [autoAcceptEnabled, setAutoAcceptEnabled] = useState(false);
  const [hideLogins, setHideLogins] = useState(false);
  const [passwordDialog, setPasswordDialog] = useState<{
    mode: "export" | "import";
    path: string;
  } | null>(null);
  const [dialogPassword, setDialogPassword] = useState("");
  const [dialogError, setDialogError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchAccounts = useCallback(async () => {
    try {
      const { loadAccounts } = await import("@/lib/tauri");
      const data = await loadAccounts();
      setAccounts(data);
    } catch {
      // Not running in Tauri
    } finally {
      setLoading(false);
    }
  }, []);

  const pollConnectivity = useCallback(async () => {
    try {
      const { probeConnectivity } = await import("@/lib/tauri");
      const status = await probeConnectivity();
      setConnectivity(status);
    } catch {
      // Not in Tauri
    }
  }, []);

  useEffect(() => {
    fetchAccounts();
    pollConnectivity();
    // Load auto-accept state and hide logins setting
    import("@/lib/tauri").then(({ isAutoAcceptEnabled, loadSetting }) => {
      isAutoAcceptEnabled().then(setAutoAcceptEnabled).catch(() => {});
      loadSetting("HideLogins", false).then((v) => setHideLogins(v as boolean)).catch(() => {});
    });
    pollRef.current = setInterval(pollConnectivity, 5000);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [fetchAccounts, pollConnectivity]);

  const handleDelete = async (username: string) => {
    try {
      const { deleteAccount } = await import("@/lib/tauri");
      await deleteAccount(username);
      await fetchAccounts();
    } catch (e) {
      console.error("Delete failed:", e);
    }
  };

  const handleKillLeague = async (includeRc: boolean) => {
    try {
      const { killLeague } = await import("@/lib/tauri");
      await killLeague(includeRc);
      setTimeout(pollConnectivity, 1000);
    } catch (e) {
      console.error("Kill failed:", e);
    }
  };

  const handleRestartLeague = async () => {
    try {
      const { restartLeague } = await import("@/lib/tauri");
      await restartLeague();
      setTimeout(pollConnectivity, 3000);
    } catch (e) {
      console.error("Restart failed:", e);
    }
  };

  const handleStartRc = async () => {
    try {
      const { startRiotClient } = await import("@/lib/tauri");
      await startRiotClient();
      setTimeout(pollConnectivity, 3000);
    } catch (e) {
      console.error("Start failed:", e);
    }
  };

  const handleExport = async () => {
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        filters: [
          { name: "LolManager (зашифрованный)", extensions: ["lolm"] },
          { name: "JSON (без шифрования)", extensions: ["json"] },
        ],
        defaultPath: "accounts-export.lolm",
      });
      if (!path) return;
      if (path.endsWith(".lolm")) {
        setDialogPassword("");
        setDialogError(null);
        setPasswordDialog({ mode: "export", path });
      } else {
        const { exportAccounts } = await import("@/lib/tauri");
        await exportAccounts(path);
      }
    } catch (e) {
      console.error("Export failed:", e);
    }
  };

  const handleToggleAutoAccept = async (enabled: boolean) => {
    setAutoAcceptEnabled(enabled);
    try {
      const { setAutoAcceptEnabled: setEnabled } = await import("@/lib/tauri");
      await setEnabled(enabled);
    } catch (e) {
      console.error("Toggle auto-accept failed:", e);
    }
  };

  const handleImport = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({
        filters: [
          { name: "LolManager (зашифрованный)", extensions: ["lolm"] },
          { name: "JSON", extensions: ["json"] },
        ],
        multiple: false,
      });
      if (!path) return;
      const filePath = path as string;
      if (filePath.endsWith(".lolm")) {
        setDialogPassword("");
        setDialogError(null);
        setPasswordDialog({ mode: "import", path: filePath });
      } else {
        const { importAccounts } = await import("@/lib/tauri");
        const count = await importAccounts(filePath);
        if (count > 0) await fetchAccounts();
      }
    } catch (e) {
      console.error("Import failed:", e);
    }
  };

  const handlePasswordSubmit = async () => {
    if (!passwordDialog || !dialogPassword.trim()) return;
    setDialogError(null);
    try {
      if (passwordDialog.mode === "export") {
        const { exportAccounts } = await import("@/lib/tauri");
        await exportAccounts(passwordDialog.path, dialogPassword);
        setPasswordDialog(null);
      } else {
        const { importAccounts } = await import("@/lib/tauri");
        const count = await importAccounts(passwordDialog.path, dialogPassword);
        if (count > 0) await fetchAccounts();
        setPasswordDialog(null);
      }
    } catch (e) {
      const msg = String(e);
      if (msg.includes("decrypt") || msg.includes("password") || msg.includes("padding")) {
        setDialogError("Неверный пароль");
      } else {
        setDialogError(msg);
      }
    }
  };

  return (
    <div className="space-y-6">
      {loginError && (
        <div className="rounded-lg border border-destructive/50 bg-destructive/10 px-4 py-2 text-sm text-destructive">
          {loginError}
        </div>
      )}

      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <h1 className="text-2xl font-bold">Аккаунты</h1>
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Автопринятие</span>
            <Switch
              checked={autoAcceptEnabled}
              onCheckedChange={handleToggleAutoAccept}
            />
          </div>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={handleExport}>
            <Download className="h-3.5 w-3.5 mr-1" /> Экспорт
          </Button>
          <Button variant="outline" size="sm" onClick={handleImport}>
            <Upload className="h-3.5 w-3.5 mr-1" /> Импорт
          </Button>
          <Link href="/add-account">
            <Button>Добавить</Button>
          </Link>
        </div>
      </div>

      {loading ? (
        <Card>
          <CardContent className="p-8 text-center text-muted-foreground">
            Загрузка...
          </CardContent>
        </Card>
      ) : accounts.length === 0 ? (
        <Card>
          <CardContent className="p-8 text-center text-muted-foreground">
            <p>Нет добавленных аккаунтов</p>
            <p className="text-sm mt-2">
              Нажмите «Добавить» чтобы начать
            </p>
          </CardContent>
        </Card>
      ) : (
        <div className="rounded-xl border border-border overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Логин</TableHead>
                <TableHead>Заметка</TableHead>
                <TableHead>Аккаунт</TableHead>
                <TableHead>Создан</TableHead>
                <TableHead className="w-[150px]">Действия</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {accounts.map((account) => (
                <TableRow key={account.Username}>
                  <TableCell className="font-medium">
                    {hideLogins ? "••••••••" : account.Username}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {account.Note || "—"}
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      {account.AvatarUrl && (
                        <img
                          src={account.AvatarUrl}
                          alt=""
                          className="w-8 h-8 rounded-full"
                        />
                      )}
                      <div>
                        <div className="text-sm">
                          {account.RiotId || account.SummonerName || "—"}
                        </div>
                        {account.RankDisplay && (
                          <Badge variant="secondary" className="text-xs">
                            {account.RankDisplay}
                          </Badge>
                        )}
                      </div>
                    </div>
                  </TableCell>
                  <TableCell className="text-muted-foreground text-sm">
                    {new Date(account.CreatedAt).toLocaleDateString("ru-RU")}
                  </TableCell>
                  <TableCell>
                    <div className="flex gap-1">
                      {loginInProgress === account.Username ? (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={async () => {
                            try {
                              const { cancelLogin } = await import("@/lib/tauri");
                              await cancelLogin();
                            } catch {}
                          }}
                        >
                          <Loader2 className="h-3 w-3 mr-1 animate-spin" />
                          Отмена
                        </Button>
                      ) : (
                        <Button
                          variant="default"
                          size="sm"
                          disabled={loginInProgress !== null}
                          onClick={async () => {
                            setLoginInProgress(account.Username);
                            setLoginError(null);
                            try {
                              const { loginToAccount } = await import("@/lib/tauri");
                              await loginToAccount(account.Username, account.EncryptedPassword);
                            } catch (e) {
                              const msg = String(e);
                              if (!msg.includes("cancelled")) {
                                setLoginError(`${account.Username}: ${msg}`);
                                setTimeout(() => setLoginError(null), 5000);
                              }
                            } finally {
                              setLoginInProgress(null);
                            }
                          }}
                        >
                          Войти
                        </Button>
                      )}
                      <Button
                        variant="destructive"
                        size="sm"
                        disabled={loginInProgress !== null}
                        onClick={() => handleDelete(account.Username)}
                      >
                        Удалить
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
      {/* Connection status bar — fixed at bottom */}
      {connectivity && (
        <div className="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-2 text-sm">
          <div className="flex items-center gap-2">
            <span
              className={`h-2.5 w-2.5 rounded-full ${
                connectivity.lcu_http_ok
                  ? "bg-green-500"
                  : connectivity.is_riot_client_running
                    ? "bg-yellow-500"
                    : "bg-red-500"
              }`}
            />
            <span className="text-muted-foreground">
              {connectivity.lcu_http_ok
                ? "League Client подключён"
                : connectivity.is_riot_client_running
                  ? "Riot Client запущен"
                  : "Клиент не запущен"}
            </span>
          </div>
          <div className="ml-auto flex gap-2">
            {!connectivity.is_riot_client_running && (
              <Button variant="outline" size="sm" onClick={handleStartRc}>
                Запустить RC
              </Button>
            )}
            {connectivity.is_league_running && (
              <>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleRestartLeague}
                >
                  Перезапустить LoL
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handleKillLeague(false)}
                >
                  Закрыть LoL
                </Button>
              </>
            )}
            {connectivity.is_riot_client_running && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => handleKillLeague(true)}
              >
                Закрыть всё
              </Button>
            )}
          </div>
        </div>
      )}

      {/* Password dialog for encrypted export/import */}
      <Dialog open={passwordDialog !== null} onOpenChange={(open) => { if (!open) setPasswordDialog(null); }}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>
              <div className="flex items-center gap-2">
                <Lock className="h-4 w-4" />
                {passwordDialog?.mode === "export" ? "Пароль для шифрования" : "Пароль для расшифровки"}
              </div>
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <Input
              type="password"
              placeholder="Введите пароль"
              value={dialogPassword}
              onChange={(e) => setDialogPassword(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handlePasswordSubmit(); }}
              autoFocus
            />
            {dialogError && (
              <p className="text-sm text-destructive">{dialogError}</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPasswordDialog(null)}>Отмена</Button>
            <Button onClick={handlePasswordSubmit} disabled={!dialogPassword.trim()}>
              {passwordDialog?.mode === "export" ? "Экспорт" : "Импорт"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
