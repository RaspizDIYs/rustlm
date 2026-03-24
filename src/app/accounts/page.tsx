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
import { AutoAcceptSwitch } from "@/components/auto-accept-provider";
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
import { Download, Upload, Loader2, Lock, ArrowUpDown, Pencil, Cloud, RefreshCw, LogIn, Trash2, KeyRound, X } from "lucide-react";
import { WithTooltip } from "@/components/ui/with-tooltip";
import type { AccountRecord, ClientConnectivityStatus, GoodLuckRiotAccount } from "@/lib/tauri";
import { LOL_SERVERS_FOR_SELECT as SERVERS } from "@/lib/lol-servers";
import { toast } from "sonner";

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
  const [glImportOpen, setGlImportOpen] = useState(false);
  const [glImportList, setGlImportList] = useState<GoodLuckRiotAccount[]>([]);
  const [glImportChecked, setGlImportChecked] = useState<Record<number, boolean>>({});
  const [glImportLoading, setGlImportLoading] = useState(false);
  const [glImportSaving, setGlImportSaving] = useState(false);
  const [glImportError, setGlImportError] = useState<string | null>(null);
  const [lcuProfileRefreshing, setLcuProfileRefreshing] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchAccounts = useCallback(async () => {
    try {
      const { loadAccounts, refreshTray } = await import("@/lib/tauri");
      const data = await loadAccounts();
      setAccounts(data);
      await refreshTray().catch(() => { });
    } catch {
      // Not running in Tauri
    } finally {
      setLoading(false);
    }
  }, []);

  const schedulePullFromLcu = useCallback((toastOnUpdate: boolean) => {
    void (async () => {
      const { invalidateLcuCache } = await import("@/lib/tauri");
      const { pullProfileFromLcuAfterLogin } = await import("@/lib/post-login-sync");
      try {
        await invalidateLcuCache();
      } catch { }
      await pullProfileFromLcuAfterLogin(fetchAccounts, {
        retries: 25,
        toastOnUpdate,
      }).catch(() => { });
    })();
  }, [fetchAccounts]);

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
    import("@/lib/tauri").then(({ loadSetting }) => {
      loadSetting("HideLogins", false).then((v) => setHideLogins(v as boolean)).catch(() => { });
    });
    pollRef.current = setInterval(pollConnectivity, 5000);

    let unlistenProgress: (() => void) | undefined;
    let unlistenGlAuth: (() => void) | undefined;
    void (async () => {
      const { isTauri } = await import("@tauri-apps/api/core");
      if (!isTauri()) return;
      const { listen } = await import("@tauri-apps/api/event");
      unlistenProgress = await listen<string>("login-progress", (event) => {
        setLoginProgress(event.payload);
      });
      await listen("cloud-sync-complete", () => {
        fetchAccounts();
      });
      unlistenGlAuth = await listen<import("@/lib/tauri").GoodLuckUser>(
        "goodluck-auth-success",
        async (event) => {
          const user = event.payload;
          if (user.riot_accounts && user.riot_accounts.length > 0) {
            try {
              const { goodluckImportProfileAccounts } = await import("@/lib/tauri");
              const result = await goodluckImportProfileAccounts(user.riot_accounts);
              if (result.imported > 0 || result.updated > 0) {
                fetchAccounts();
              }
              if (result.updated > 0) {
                const pairs = result.updated_pairs
                  .map(([old, next]) => `${old} → ${next}`)
                  .join(", ");
                toast.info(`GoodLuck: обновлено ${result.updated} аккаунт(а): ${pairs}`);
              }
              if (result.imported > 0) {
                toast.success(`GoodLuck: импортировано ${result.imported} новых аккаунт(а)`);
              }
            } catch { }
          }
        },
      );
    })();

    const onAccountsReload = () => {
      void fetchAccounts();
    };
    window.addEventListener("rustlm-accounts-reload", onAccountsReload);

    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
      unlistenProgress?.();
      unlistenGlAuth?.();
      window.removeEventListener("rustlm-accounts-reload", onAccountsReload);
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
        if (count > 0) {
          await fetchAccounts();
          schedulePullFromLcu(false);
        }
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
    // For GoodLuck-imported accounts, clear the gl: placeholder so user enters real login
    setEditUsername(account.Username.startsWith("gl:") ? "" : account.Username);
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
        Puuid: "",
        RankIconUrl: "",
        Server: addServer,
      });
      setAddOpen(false);
      setAddUsername("");
      setAddPassword("");
      setAddNote("");
      setAddServer("");
      await fetchAccounts();
      schedulePullFromLcu(true);
    } catch (e) {
      console.error("Add failed:", e);
    } finally {
      setAddSaving(false);
    }
  };

  const fetchGlProfileRiotAccounts = useCallback(async () => {
    setGlImportLoading(true);
    setGlImportError(null);
    try {
      const { goodluckRefreshProfile, goodluckGetProfileAccounts } = await import("@/lib/tauri");
      await goodluckRefreshProfile();
      const list = await goodluckGetProfileAccounts();
      setGlImportList(list);
      const next: Record<number, boolean> = {};
      for (let i = 0; i < list.length; i++) next[i] = false;
      setGlImportChecked(next);
    } catch (e) {
      setGlImportError(String(e));
      setGlImportList([]);
      setGlImportChecked({});
    } finally {
      setGlImportLoading(false);
    }
  }, []);

  const openGlImportDialog = () => {
    setGlImportOpen(true);
    setGlImportList([]);
    setGlImportChecked({});
    setGlImportError(null);
    void fetchGlProfileRiotAccounts();
  };

  const handleGlImportSubmit = async () => {
    const picked = glImportList.filter((_, i) => glImportChecked[i]);
    if (picked.length === 0) return;
    setGlImportSaving(true);
    setGlImportError(null);
    try {
      const { goodluckImportProfileAccounts } = await import("@/lib/tauri");
      const result = await goodluckImportProfileAccounts(picked);
      setGlImportOpen(false);
      await fetchAccounts();
      schedulePullFromLcu(false);

      const parts: string[] = [];
      if (result.imported > 0) parts.push(`${result.imported} новых`);
      if (result.updated > 0) parts.push(`${result.updated} обновлено`);
      if (result.skipped > 0) parts.push(`${result.skipped} пропущено`);

      if (result.updated > 0) {
        const pairs = result.updated_pairs.map(([old, next]) => `${old} → ${next}`).join(", ");
        toast.info(`Обновлены аккаунты со сменой ника: ${pairs}`);
      } else if (result.imported > 0) {
        toast.success(`Импорт из GoodLuck: ${parts.join(", ")}`);
      } else {
        toast(`Все аккаунты уже в списке (${result.skipped} пропущено)`);
      }
    } catch (e) {
      setGlImportError(String(e));
    } finally {
      setGlImportSaving(false);
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
      schedulePullFromLcu(true);
    } catch (e) {
      console.error("Edit failed:", e);
    } finally {
      setEditSaving(false);
    }
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-col gap-4">
      {loginError && (
        <div className="shrink-0 rounded-lg border border-destructive/50 bg-destructive/10 px-4 py-2 text-sm text-destructive">
          {loginError}
        </div>
      )}

      <div className="sticky top-0 z-10 flex shrink-0 flex-col gap-3 border-b border-border/60 bg-background/95 pb-3 backdrop-blur-sm sm:flex-row sm:items-start sm:justify-between sm:border-0 sm:bg-transparent sm:pb-0 sm:backdrop-blur-none">
        <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2">
          <h1 className="shrink-0 text-2xl font-bold">Аккаунты</h1>
          <AutoAcceptSwitch />
        </div>
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <div className="inline-flex overflow-hidden rounded-md border border-input shadow-xs">
            <WithTooltip label="Экспорт аккаунтов в файл">
              <Button
                variant="outline"
                size="sm"
                className="rounded-none border-0 shadow-none"
                onClick={handleExport}
              >
                <Download className="h-3.5 w-3.5" />
              </Button>
            </WithTooltip>
            <div className="w-px shrink-0 bg-border" aria-hidden />
            <WithTooltip label="Импорт аккаунтов из файла">
              <Button
                variant="outline"
                size="sm"
                className="rounded-none border-0 shadow-none"
                onClick={handleImport}
              >
                <Upload className="h-3.5 w-3.5" />
              </Button>
            </WithTooltip>
          </div>
          <Button variant="outline" size="sm" onClick={openGlImportDialog}>
            <Cloud className="h-3.5 w-3.5 mr-1" /> Из GoodLuck
          </Button>
          <WithTooltip label="Ник, аватар, ранг из текущей сессии League (клиент должен быть запущен и залогинен)">
            <Button
              variant="outline"
              size="sm"
              disabled={lcuProfileRefreshing}
              onClick={async () => {
                setLcuProfileRefreshing(true);
                try {
                  const { refreshAccountProfileFromLcu } = await import("@/lib/tauri");
                  const { notifyCloudAfterGoodluckSession } = await import("@/lib/post-login-sync");
                  const r = await refreshAccountProfileFromLcu();
                  await fetchAccounts();
                  await notifyCloudAfterGoodluckSession();
                  if (r.updated) toast.success(r.message);
                  else toast.info(r.message);
                } catch (e) {
                  toast.error(String(e));
                } finally {
                  setLcuProfileRefreshing(false);
                }
              }}
            >
              <RefreshCw className={`h-3.5 w-3.5 mr-1 ${lcuProfileRefreshing ? "animate-spin" : ""}`} />{" "}
              Из клиента
            </Button>
          </WithTooltip>
          <Button onClick={() => setAddOpen(true)}>Добавить</Button>
        </div>
      </div>

      <div className="min-h-0 min-w-0 flex-1 overflow-auto">
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
                  <TableHead className="w-[168px] min-w-[168px]">Действия</TableHead>
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

                  const NO_SERVER_LABEL = "Без сервера";
                  const serverOrderRank = (name: string): number => {
                    if (name === NO_SERVER_LABEL) return 3;
                    const u = name.toUpperCase();
                    if (u === "RU") return 0;
                    if (u === "EUW") return 1;
                    return 2;
                  };
                  const serverEntries = [...servers.entries()].sort(([a], [b]) => {
                    const ra = serverOrderRank(a);
                    const rb = serverOrderRank(b);
                    if (ra !== rb) return ra - rb;
                    return a.localeCompare(b, "ru");
                  });

                  const rows: React.ReactNode[] = [];
                  for (const [server, group] of serverEntries) {
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
                      const isNotActivated = !account.EncryptedPassword;
                      const displayUsername = account.Username.startsWith("gl:")
                        ? account.Username.slice(3)
                        : account.Username;
                      rows.push(
                        <TableRow key={account.Username} className={isNotActivated ? "opacity-70" : ""}>
                          <TableCell className="font-medium">
                            <div className="flex items-center gap-2">
                              <span>{hideLogins ? "••••••••" : displayUsername}</span>
                              {isNotActivated && (
                                <Badge variant="outline" className="text-xs text-amber-500 border-amber-500/50">
                                  Не активирован
                                </Badge>
                              )}
                            </div>
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
                            <div className="flex items-center gap-1">
                              {loginInProgress === account.Username ? (
                                <>
                                  <WithTooltip label="Отменить вход">
                                    <Button
                                      variant="outline"
                                      size="icon"
                                      className="h-9 w-9"
                                      onClick={async () => {
                                        try {
                                          const { cancelLogin } = await import("@/lib/tauri");
                                          await cancelLogin();
                                        } catch { }
                                      }}
                                      aria-label="Отменить вход"
                                    >
                                      <X className="h-4 w-4" />
                                    </Button>
                                  </WithTooltip>
                                  <Loader2 className="h-4 w-4 shrink-0 animate-spin text-muted-foreground" aria-hidden />
                                  {loginProgress && (
                                    <span className="max-w-32 truncate text-xs text-muted-foreground">{loginProgress}</span>
                                  )}
                                </>
                              ) : isNotActivated ? (
                                <WithTooltip label="Активировать — задать пароль Riot">
                                  <Button
                                    variant="outline"
                                    size="icon"
                                    className="h-9 w-9 text-amber-600 border-amber-500/50 hover:bg-amber-500/10"
                                    disabled={loginInProgress !== null}
                                    aria-label="Активировать"
                                    onClick={() => openEditDialog(account)}
                                  >
                                    <KeyRound className="h-4 w-4" />
                                  </Button>
                                </WithTooltip>
                              ) : (
                                <WithTooltip label="Войти в аккаунт">
                                  <Button
                                    variant="default"
                                    size="icon"
                                    className="h-9 w-9"
                                    disabled={loginInProgress !== null}
                                    aria-label="Войти"
                                    onClick={async () => {
                                      setLoginInProgress(account.Username);
                                      setLoginProgress("");
                                      setLoginError(null);
                                      try {
                                        const {
                                          loginToAccount,
                                          invalidateLcuCache,
                                        } = await import("@/lib/tauri");
                                        await loginToAccount(account.Username);
                                        try {
                                          await invalidateLcuCache();
                                        } catch { }
                                        const { pullProfileFromLcuAfterLogin } = await import("@/lib/post-login-sync");
                                        void pullProfileFromLcuAfterLogin(fetchAccounts, {
                                          retries: 25,
                                          toastOnUpdate: true,
                                        }).catch(() => { });
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
                                    <LogIn className="h-4 w-4" />
                                  </Button>
                                </WithTooltip>
                              )}
                              {!isNotActivated && (
                                <WithTooltip label="Редактировать аккаунт">
                                  <Button
                                    variant="outline"
                                    size="icon"
                                    className="h-9 w-9"
                                    disabled={loginInProgress !== null}
                                    aria-label="Редактировать"
                                    onClick={() => openEditDialog(account)}
                                  >
                                    <Pencil className="h-4 w-4" />
                                  </Button>
                                </WithTooltip>
                              )}
                              <WithTooltip label="Удалить аккаунт">
                                <Button
                                  variant="destructive"
                                  size="icon"
                                  className="h-9 w-9"
                                  disabled={loginInProgress !== null}
                                  aria-label="Удалить"
                                  onClick={() => setDeleteTarget(account.Username)}
                                >
                                  <Trash2 className="h-4 w-4" />
                                </Button>
                              </WithTooltip>
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
      </div>
      {connectivity && (
        <div className="sticky bottom-0 z-10 mt-2 flex w-full min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-2 rounded-xl border border-border bg-card/95 px-4 py-2 text-sm shadow-md backdrop-blur-sm">
          <div className="flex min-w-0 items-center gap-2">
            <span
              className={`h-2.5 w-2.5 shrink-0 rounded-full ${connectivity.lcu_http_ok
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
          <div className="flex min-w-0 flex-wrap items-center gap-2">
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

      <Dialog open={glImportOpen} onOpenChange={(open) => { if (!open) setGlImportOpen(false); }}>
        <DialogContent className="sm:max-w-md max-h-[85vh] flex flex-col gap-0">
          <DialogHeader className="flex flex-row items-center justify-between gap-2 space-y-0">
            <DialogTitle className="flex-1">Импорт из GoodLuck</DialogTitle>
            <WithTooltip label="GET /me — актуальный список Riot с GoodLuck">
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="shrink-0 gap-1"
                disabled={glImportLoading || glImportSaving}
                onClick={() => void fetchGlProfileRiotAccounts()}
              >
                <RefreshCw className={`h-3.5 w-3.5 ${glImportLoading ? "animate-spin" : ""}`} />
                С сервера
              </Button>
            </WithTooltip>
          </DialogHeader>
          <div className="space-y-3 overflow-y-auto py-2">
            {glImportLoading ? (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Загрузка профиля…
              </div>
            ) : glImportList.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {glImportError
                  ? glImportError
                  : "В профиле GoodLuck нет Riot-аккаунтов или нужно войти через GoodLuck в сайдбаре."}
              </p>
            ) : (
              <div className="space-y-2">
                {glImportList.map((acc, idx) => (
                  <label
                    key={`${acc.riot_id}-${idx}`}
                    className="flex cursor-pointer items-start gap-2 rounded-md border border-border p-2 text-sm"
                  >
                    <input
                      type="checkbox"
                      className="mt-1"
                      checked={Boolean(glImportChecked[idx])}
                      onChange={(e) =>
                        setGlImportChecked((prev) => ({ ...prev, [idx]: e.target.checked }))
                      }
                    />
                    <div className="min-w-0">
                      <div className="font-medium truncate">{acc.riot_id || "—"}</div>
                      <div className="text-xs text-muted-foreground">
                        {acc.server}
                        {acc.rank ? ` · ${acc.rank}` : ""}
                      </div>
                    </div>
                  </label>
                ))}
              </div>
            )}
            {glImportError && glImportList.length > 0 && (
              <p className="text-sm text-destructive">{glImportError}</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setGlImportOpen(false)} disabled={glImportSaving}>
              Закрыть
            </Button>
            <Button
              onClick={handleGlImportSubmit}
              disabled={
                glImportSaving ||
                glImportLoading ||
                glImportList.length === 0 ||
                !Object.values(glImportChecked).some(Boolean)
              }
            >
              {glImportSaving ? "Импорт…" : "Импортировать"}
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
                <SelectTrigger className="w-full">
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
            <DialogTitle>
              {editAccount && !editAccount.EncryptedPassword ? "Активация аккаунта" : "Редактировать аккаунт"}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            {editAccount && !editAccount.EncryptedPassword && editAccount.RiotId && (
              <div className="rounded-md bg-muted/50 p-3">
                <div className="text-sm font-medium">{editAccount.RiotId}</div>
                <div className="text-xs text-muted-foreground">
                  {editAccount.Server}
                  {editAccount.RankDisplay && ` · ${editAccount.RankDisplay}`}
                </div>
              </div>
            )}
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Логин</label>
              <Input
                placeholder={editAccount && !editAccount.EncryptedPassword ? "Логин Riot аккаунта" : ""}
                value={editUsername}
                onChange={(e) => setEditUsername(e.target.value)}
                autoFocus
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">
                {editAccount && !editAccount.EncryptedPassword ? "Пароль" : "Новый пароль"}
              </label>
              <Input
                type="password"
                placeholder={editAccount && !editAccount.EncryptedPassword ? "Пароль Riot аккаунта" : "Оставьте пустым, чтобы не менять"}
                value={editPassword}
                onChange={(e) => setEditPassword(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Сервер</label>
              <Select value={editServer} onValueChange={(v) => v && setEditServer(v)}>
                <SelectTrigger className="w-full">
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
            <Button
              onClick={handleEditSubmit}
              disabled={editSaving || !editUsername || (editAccount != null && !editAccount.EncryptedPassword && !editPassword)}
            >
              {editSaving ? "Сохранение..." : editAccount && !editAccount.EncryptedPassword ? "Активировать" : "Сохранить"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

    </div>
  );
}
