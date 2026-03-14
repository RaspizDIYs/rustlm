"use client";

import { useEffect, useState, useCallback, useRef } from "react";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Download, Upload, Loader2, Lock, ArrowUpDown, Pencil } from "lucide-react";
import type { AccountRecord, ClientConnectivityStatus } from "@/lib/tauri";

const SERVERS = [
  { code: "EUW", name: "EU West" },
  { code: "EUNE", name: "EU Nordic & East" },
  { code: "NA", name: "North America" },
  { code: "KR", name: "Korea" },
  { code: "RU", name: "Russia" },
  { code: "TR", name: "Turkey" },
  { code: "BR", name: "Brazil" },
  { code: "JP", name: "Japan" },
  { code: "LAN", name: "Latin America North" },
  { code: "LAS", name: "Latin America South" },
  { code: "OCE", name: "Oceania" },
];

const RANK_ORDER: Record<string, number> = {
  IRON: 1, BRONZE: 2, SILVER: 3, GOLD: 4, PLATINUM: 5,
  EMERALD: 6, DIAMOND: 7, MASTER: 8, GRANDMASTER: 9, CHALLENGER: 10,
};
const DIVISION_ORDER: Record<string, number> = { IV: 1, III: 2, II: 3, I: 4 };

function rankToNumber(rankDisplay: string): number {
  if (!rankDisplay) return 0;
  const parts = rankDisplay.toUpperCase().split(" ");
  const tier = RANK_ORDER[parts[0]] || 0;
  const div = DIVISION_ORDER[parts[1]] || 0;
  return tier * 10 + div;
}

type SortMode = "default" | "rank-asc" | "rank-desc";

export default function AccountsPage() {
  const [accounts, setAccounts] = useState<AccountRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [loginInProgress, setLoginInProgress] = useState<string | null>(null);
  const [loginProgress, setLoginProgress] = useState<string>("");
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
  const [sortMode, setSortMode] = useState<SortMode>("default");
  const [addOpen, setAddOpen] = useState(false);
  const [addUsername, setAddUsername] = useState("");
  const [addPassword, setAddPassword] = useState("");
  const [addNote, setAddNote] = useState("");
  const [addServer, setAddServer] = useState("");
  const [addSaving, setAddSaving] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [editAccount, setEditAccount] = useState<AccountRecord | null>(null);
  const [editUsername, setEditUsername] = useState("");
  const [editPassword, setEditPassword] = useState("");
  const [editNote, setEditNote] = useState("");
  const [editServer, setEditServer] = useState("");
  const [editSaving, setEditSaving] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchAccounts = useCallback(async () => {
    try {
      const { loadAccounts, refreshTray } = await import("@/lib/tauri");
      const data = await loadAccounts();
      setAccounts(data);
      await refreshTray().catch(() => {});
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

    // Sync auto-accept state when toggled from tray
    const onAutoAcceptSync = (e: Event) => {
      setAutoAcceptEnabled((e as CustomEvent<boolean>).detail);
    };
    window.addEventListener("autoAcceptSync", onAutoAcceptSync);

    // Listen for login progress events from backend
    let unlistenProgress: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<string>("login-progress", (event) => {
        setLoginProgress(event.payload);
      }).then((fn) => { unlistenProgress = fn; });
    });

    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
      window.removeEventListener("autoAcceptSync", onAutoAcceptSync);
      unlistenProgress?.();
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
      const { setAutoAcceptEnabled: setEnabled, refreshTray } = await import("@/lib/tauri");
      await setEnabled(enabled);
      await refreshTray().catch(() => {});
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

  const openEditDialog = (account: AccountRecord) => {
    setEditAccount(account);
    setEditUsername(account.Username);
    setEditPassword("");
    setEditNote(account.Note);
    setEditServer(account.Server);
  };

  const handleAddSubmit = async () => {
    if (!addUsername || !addPassword) return;
    setAddSaving(true);
    try {
      const { protectPassword, saveAccount } = await import("@/lib/tauri");
      const encryptedPassword = await protectPassword(addPassword);
      await saveAccount({
        Username: addUsername,
        EncryptedPassword: encryptedPassword,
        Note: addNote,
        CreatedAt: new Date().toISOString(),
        AvatarUrl: "",
        SummonerName: "",
        Rank: "",
        RankDisplay: "",
        RiotId: "",
        RankIconUrl: "",
        Server: addServer,
      });
      setAddOpen(false);
      setAddUsername("");
      setAddPassword("");
      setAddNote("");
      setAddServer("");
      await fetchAccounts();
    } catch (e) {
      console.error("Add failed:", e);
    } finally {
      setAddSaving(false);
    }
  };

  const handleEditSubmit = async () => {
    if (!editAccount || !editUsername) return;
    setEditSaving(true);
    try {
      const { protectPassword, saveAccount, deleteAccount: delAcc } = await import("@/lib/tauri");
      const encryptedPassword = editPassword
        ? await protectPassword(editPassword)
        : editAccount.EncryptedPassword;

      // If username changed, delete the old record first
      if (editUsername !== editAccount.Username) {
        await delAcc(editAccount.Username);
      }

      await saveAccount({
        ...editAccount,
        Username: editUsername,
        EncryptedPassword: encryptedPassword,
        Note: editNote,
        Server: editServer,
      });
      setEditAccount(null);
      await fetchAccounts();
    } catch (e) {
      console.error("Edit failed:", e);
    } finally {
      setEditSaving(false);
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
          <Button onClick={() => setAddOpen(true)}>Добавить</Button>
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
                <TableHead>
                  <button
                    className="flex items-center gap-1 hover:text-foreground transition-colors"
                    onClick={() => setSortMode((m) => m === "default" ? "rank-desc" : m === "rank-desc" ? "rank-asc" : "default")}
                  >
                    Аккаунт
                    <ArrowUpDown className={`h-3 w-3 ${sortMode !== "default" ? "text-primary" : "opacity-50"}`} />
                  </button>
                </TableHead>
                <TableHead>Сервер</TableHead>
                <TableHead className="w-[150px]">Действия</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(() => {
                let sorted = [...accounts];
                if (sortMode === "rank-desc") {
                  sorted.sort((a, b) => rankToNumber(b.RankDisplay) - rankToNumber(a.RankDisplay));
                } else if (sortMode === "rank-asc") {
                  sorted.sort((a, b) => rankToNumber(a.RankDisplay) - rankToNumber(b.RankDisplay));
                }

                const servers = new Map<string, AccountRecord[]>();
                for (const acc of sorted) {
                  const srv = acc.Server || "Без сервера";
                  if (!servers.has(srv)) servers.set(srv, []);
                  servers.get(srv)!.push(acc);
                }

                const hasMultipleServers = servers.size > 1 || (servers.size === 1 && !servers.has("Без сервера"));

                const rows: React.ReactNode[] = [];
                for (const [server, group] of servers) {
                  if (hasMultipleServers) {
                    rows.push(
                      <TableRow key={`srv-${server}`}>
                        <TableCell colSpan={5} className="bg-muted/30 py-1.5 px-3">
                          <span className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                            {server} ({group.length})
                          </span>
                        </TableCell>
                      </TableRow>
                    );
                  }
                  for (const account of group) {
                    rows.push(
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
                                onError={(e) => {
                                  const img = e.currentTarget;
                                  img.style.display = "none";
                                }}
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
                          {account.Server || "—"}
                        </TableCell>
                        <TableCell>
                          <div className="flex gap-1">
                            {loginInProgress === account.Username ? (
                              <div className="flex items-center gap-2">
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
                                {loginProgress && (
                                  <span className="text-xs text-muted-foreground">{loginProgress}</span>
                                )}
                              </div>
                            ) : (
                              <Button
                                variant="default"
                                size="sm"
                                disabled={loginInProgress !== null}
                                onClick={async () => {
                                  setLoginInProgress(account.Username);
                                  setLoginProgress("");
                                  setLoginError(null);
                                  try {
                                    const { loginToAccount, detectServer, getAccountInfo, saveAccount } = await import("@/lib/tauri");
                                    await loginToAccount(account.Username, account.EncryptedPassword);
                                    // Fetch account info & server after login (with retries since LCU may not be ready)
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
                                        fetchAccounts();
                                      }
                                    };
                                    // Run in background so login button unblocks immediately
                                    fetchInfo(3).catch(() => {});
                                  } catch (e) {
                                    const msg = String(e);
                                    if (!msg.includes("cancelled")) {
                                      setLoginError(`${account.Username}: ${msg}`);
                                      setTimeout(() => setLoginError(null), 5000);
                                    }
                                  } finally {
                                    setLoginInProgress(null);
                                    setLoginProgress("");
                                  }
                                }}
                              >
                                Войти
                              </Button>
                            )}
                            <Button
                              variant="outline"
                              size="sm"
                              disabled={loginInProgress !== null}
                              onClick={() => openEditDialog(account)}
                            >
                              <Pencil className="h-3 w-3" />
                            </Button>
                            <Button
                              variant="destructive"
                              size="sm"
                              disabled={loginInProgress !== null}
                              onClick={() => setDeleteTarget(account.Username)}
                            >
                              Удалить
                            </Button>
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  }
                }
                return rows;
              })()}
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

      {/* Add account dialog */}
      <Dialog open={addOpen} onOpenChange={(open) => { if (!open) setAddOpen(false); }}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Добавить аккаунт</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Логин</label>
              <Input
                placeholder="Введите логин"
                value={addUsername}
                onChange={(e) => setAddUsername(e.target.value)}
                autoFocus
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Пароль</label>
              <Input
                type="password"
                placeholder="Введите пароль"
                value={addPassword}
                onChange={(e) => setAddPassword(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Сервер</label>
              <Select value={addServer} onValueChange={(v) => v && setAddServer(v)}>
                <SelectTrigger>
                  <SelectValue placeholder="Выберите сервер" />
                </SelectTrigger>
                <SelectContent>
                  {SERVERS.map((s) => (
                    <SelectItem key={s.code} value={s.code}>
                      {s.code} — {s.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Заметка</label>
              <Input
                placeholder="Необязательная заметка"
                value={addNote}
                onChange={(e) => setAddNote(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAddOpen(false)}>Отмена</Button>
            <Button onClick={handleAddSubmit} disabled={addSaving || !addUsername || !addPassword}>
              {addSaving ? "Сохранение..." : "Добавить"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirmation dialog */}
      <Dialog open={deleteTarget !== null} onOpenChange={(open) => { if (!open) setDeleteTarget(null); }}>
        <DialogContent className="sm:max-w-xs">
          <DialogHeader>
            <DialogTitle>Удалить аккаунт?</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            Аккаунт <span className="font-medium text-foreground">{deleteTarget}</span> будет удалён. Это действие нельзя отменить.
          </p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>Отмена</Button>
            <Button
              variant="destructive"
              onClick={async () => {
                if (deleteTarget) {
                  await handleDelete(deleteTarget);
                  setDeleteTarget(null);
                }
              }}
            >
              Удалить
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit account dialog */}
      <Dialog open={editAccount !== null} onOpenChange={(open) => { if (!open) setEditAccount(null); }}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Редактировать аккаунт</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Логин</label>
              <Input
                value={editUsername}
                onChange={(e) => setEditUsername(e.target.value)}
                autoFocus
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Новый пароль</label>
              <Input
                type="password"
                placeholder="Оставьте пустым, чтобы не менять"
                value={editPassword}
                onChange={(e) => setEditPassword(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Сервер</label>
              <Select value={editServer} onValueChange={(v) => v && setEditServer(v)}>
                <SelectTrigger>
                  <SelectValue placeholder="Выберите сервер" />
                </SelectTrigger>
                <SelectContent>
                  {SERVERS.map((s) => (
                    <SelectItem key={s.code} value={s.code}>
                      {s.code} — {s.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Заметка</label>
              <Input
                placeholder="Необязательная заметка"
                value={editNote}
                onChange={(e) => setEditNote(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditAccount(null)}>Отмена</Button>
            <Button onClick={handleEditSubmit} disabled={editSaving || !editUsername}>
              {editSaving ? "Сохранение..." : "Сохранить"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
