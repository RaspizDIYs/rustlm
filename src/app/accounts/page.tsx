"use client";
import { RankBadge } from "@/components/rank-badge";
import { HighlightText } from "@/components/highlight-text";
import { LmFriends } from "@/components/lm-friends";

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
import {
  Download,
  Upload,
  Loader2,
  Lock,
  ArrowUpDown,
  Pencil,
  Cloud,
  RefreshCw,
  LogIn,
  Trash2,
  KeyRound,
  X,
  Pin,
  PinOff,
  ChevronDown,
  Share2,
  ShieldOff,
  UserPlus,
  Search,
  Plus,
  Ellipsis,
  History,
  ChevronUp,
} from "lucide-react";
import { WithTooltip } from "@/components/ui/with-tooltip";
import type {
  AccountRecord,
  ClientConnectivityStatus,
  GoodLuckFriend,
  GoodLuckRiotAccount,
  MatchSummary,
  SharedAccount,
  SharedAccountsResponse,
} from "@/lib/tauri";
import { AccountMatches } from "@/components/account-matches";
import {
  visibleAccounts,
  type AccountSort,
  type AccountSortKey,
} from "@/lib/accounts-view";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";

import { LOL_SERVERS_FOR_SELECT as SERVERS } from "@/lib/lol-servers";
import { toast } from "sonner";

const hasValidPuuid = (value: string) => /^[A-Za-z0-9_-]{78}$/.test(value);


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
  const [sort, setSort] = useState<AccountSort | null>(null);
  const [query, setQuery] = useState("");
  const cycleSort = (key: AccountSortKey) => setSort(current => ({key, descending: current?.key === key ? !current.descending : false}));
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
  const [expandedAccount, setExpandedAccount] = useState<string | null>(null);
  const [matchesByAccount, setMatchesByAccount] = useState<Record<string, MatchSummary[]>>({});
  const [matchesLoading, setMatchesLoading] = useState<string | null>(null);
  const [matchesError, setMatchesError] = useState<Record<string, string>>({});
  const [sharedAccounts, setSharedAccounts] = useState<SharedAccountsResponse>({ available_to_me: [], shared_by_me: [] });
  const [sharedLoading, setSharedLoading] = useState(false);
  const [shareTarget, setShareTarget] = useState<AccountRecord | null>(null);
  const [friendsTab, setFriendsTab] = useState(false);
  const [friends, setFriends] = useState<GoodLuckFriend[]>([]);
  const [shareFriend, setShareFriend] = useState("");
  const [shareDuration, setShareDuration] = useState("168");
  const [sharePassword, setSharePassword] = useState("");
  const [shareSaving, setShareSaving] = useState(false);
  const [shareError, setShareError] = useState<string | null>(null);
  const [friendSearchOpen, setFriendSearchOpen] = useState(false);
  const [friendQuery, setFriendQuery] = useState("");
  const [friendResults, setFriendResults] = useState<GoodLuckFriend[]>([]);
  const [friendSearchLoading, setFriendSearchLoading] = useState(false);
  const [friendRequestSending, setFriendRequestSending] = useState<string | null>(null);
  const [friendNotice, setFriendNotice] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const accountsRequest = useRef(0);
  const connectivityPending = useRef(false);
  const sharedPending = useRef(false);

  const fetchSharedAccounts = useCallback(async (showSpinner = false) => {
    if (sharedPending.current) return;
    sharedPending.current = true;
    if (showSpinner) setSharedLoading(true);
    try {
      const { goodluckGetSharedAccounts } = await import("@/lib/tauri");
      const data = await goodluckGetSharedAccounts();
      const now = Date.now();
      setSharedAccounts({
        available_to_me: data.available_to_me.filter((a) => !a.expires_at || Date.parse(a.expires_at) > now),
        shared_by_me: data.shared_by_me.filter((a) => !a.expires_at || Date.parse(a.expires_at) > now),
      });
    } catch {
      // GoodLuck is optional; local accounts remain available offline.
    } finally {
      sharedPending.current = false;
      if (showSpinner) setSharedLoading(false);
    }
  }, []);

  const fetchAccounts = useCallback(async () => {
    const request = ++accountsRequest.current;
    try {
      const { loadAccounts, refreshTray } = await import("@/lib/tauri");
      const data = await loadAccounts();
      if (request !== accountsRequest.current) return;
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
    if (connectivityPending.current) return;
    connectivityPending.current = true;
    try {
      const { probeConnectivity } = await import("@/lib/tauri");
      const status = await probeConnectivity();
      setConnectivity((old) => JSON.stringify(old) === JSON.stringify(status) ? old : status);
    } catch {
      // Not in Tauri
    } finally { connectivityPending.current = false; }
  }, []);

  const toggleAccountMatches = useCallback(
    async (account: AccountRecord) => {
      const key = account.Username;
      if (expandedAccount === key) {
        setExpandedAccount(null);
        return;
      }
      setExpandedAccount(key);
      if (matchesByAccount[key] || matchesLoading === key) return;
      if (!hasValidPuuid(account.Puuid)) {
        setMatchesError((prev) => ({
          ...prev,
          [key]: "У аккаунта нет корректного PUUID — обнови профиль кнопкой «Из клиента».",
        }));
        return;
      }
      setMatchesLoading(key);
      setMatchesError((prev) => {
        const next = { ...prev };
        delete next[key];
        return next;
      });
      try {
        const { getRecentMatches } = await import("@/lib/tauri");
        const matches = await getRecentMatches(account.Puuid, account.Server, 20);
        setMatchesByAccount((prev) => ({ ...prev, [key]: matches }));
      } catch (e) {
        setMatchesError((prev) => ({ ...prev, [key]: String(e) }));
      } finally {
        setMatchesLoading(null);
      }
    },
    [expandedAccount, matchesByAccount, matchesLoading]
  );

  useEffect(() => {
    fetchAccounts();
    void fetchSharedAccounts();
    pollConnectivity();
    import("@/lib/tauri").then(({ loadSetting }) => {
      loadSetting("HideLogins", false).then((v) => setHideLogins(v as boolean)).catch(() => { });
    });
    pollRef.current = setInterval(() => { if (!document.hidden) void pollConnectivity(); }, 5000);
    const sharedPoll = setInterval(() => { if (!document.hidden) void fetchSharedAccounts(); }, 30000);

    let disposed = false;
    const onVisible = () => { if (!document.hidden) { void pollConnectivity(); void fetchSharedAccounts(); } };
    document.addEventListener("visibilitychange", onVisible);
    let unlistenProgress: (() => void) | undefined;
    let unlistenGlAuth: (() => void) | undefined;
    let unlistenCloud: (() => void) | undefined;
    let unlistenShared: (() => void) | undefined;
    void (async () => {
      const { isTauri } = await import("@tauri-apps/api/core");
      if (!isTauri()) return;
      const { listen } = await import("@tauri-apps/api/event");
      unlistenProgress = await listen<string>("login-progress", (event) => {
        setLoginProgress(event.payload);
      });
      unlistenCloud = await listen("cloud-sync-complete", () => {
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
      unlistenShared = await listen("shared-accounts-changed", () => void fetchSharedAccounts());
      if (disposed) { unlistenProgress?.(); unlistenGlAuth?.(); unlistenCloud?.(); unlistenShared?.(); }
    })();

    const onAccountsReload = () => {
      void fetchAccounts();
    };
    window.addEventListener("rustlm-accounts-reload", onAccountsReload);

    return () => {
      disposed = true;
      document.removeEventListener("visibilitychange", onVisible);
      if (pollRef.current) clearInterval(pollRef.current);
      clearInterval(sharedPoll);
      unlistenProgress?.();
      unlistenGlAuth?.();
      unlistenCloud?.();
      unlistenShared?.();
      window.removeEventListener("rustlm-accounts-reload", onAccountsReload);
    };
  }, [fetchAccounts, fetchSharedAccounts, pollConnectivity]);

  const handleDelete = async (username: string) => {
    ++accountsRequest.current;
    try {
      const { deleteAccount } = await import("@/lib/tauri");
      await deleteAccount(username);
      await fetchAccounts();
    } catch (e) {
      toast.error(`Не удалось удалить аккаунт: ${String(e)}`);
    }
  };

  const handleTogglePin = async (account: AccountRecord) => {
    try {
      const { setAccountPinned } = await import("@/lib/tauri");
      await setAccountPinned(account.Username, !account.Pinned);
      await fetchAccounts();
    } catch (e) {
      console.error("Pin toggle failed:", e);
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
        SummonerLevel: 0,
        Pinned: false,
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
      const result = await goodluckImportProfileAccounts(picked, true);
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

  const openShareDialog = async (account: AccountRecord, recipient?: GoodLuckFriend) => {
    if (!account.RiotId) {
      toast.error("Сначала обнови аккаунт «Из клиента», чтобы привязать Riot ID для безопасного отзыва.");
      return;
    }
    setShareTarget(account);
    setSharePassword("");
    setShareFriend("");
    setShareDuration("168");
    setShareError(null);
    setFriendQuery("");
    setFriendResults([]);
    setFriendNotice(null);
    setFriendSearchOpen(false);
    try {
      const { goodluckGetFriends } = await import("@/lib/tauri");
      const data = await goodluckGetFriends();
      setFriends(recipient && !data.some(f=>f.id===recipient.id) ? [recipient,...data] : data);
      if (recipient) setShareFriend(recipient.id);
      setFriendSearchOpen(data.length === 0 && !recipient);
    } catch (e) {
      setFriends(recipient ? [recipient] : []);
      if (recipient) setShareFriend(recipient.id);
      setFriendSearchOpen(!recipient);
      if (!recipient) setShareError(`Не удалось загрузить друзей GoodLuck: ${String(e)}`);
    }
  };

  const handleFriendSearch = async () => {
    if (friendQuery.trim().length < 2) {
      setShareError("Введи минимум 2 символа для поиска.");
      return;
    }
    setFriendSearchLoading(true);
    setShareError(null);
    setFriendNotice(null);
    try {
      const { goodluckSearchUsers } = await import("@/lib/tauri");
      const data = await goodluckSearchUsers(friendQuery.trim());
      setFriendResults(data.filter((user) => !friends.some((friend) => friend.id === user.id)));
      if (data.length === 0) setFriendNotice("Пользователи не найдены.");
    } catch (e) {
      setShareError(`Не удалось найти пользователя: ${String(e)}`);
    } finally {
      setFriendSearchLoading(false);
    }
  };

  const handleSendFriendRequest = async (user: GoodLuckFriend) => {
    setFriendRequestSending(user.id);
    setShareError(null);
    setFriendNotice(null);
    try {
      const { goodluckSendFriendRequest } = await import("@/lib/tauri");
      await goodluckSendFriendRequest(user.id);
      setFriendResults((prev) => prev.filter((item) => item.id !== user.id));
      setFriendNotice(`Заявка отправлена: ${user.display_name}. Друг появится в списке после её принятия.`);
    } catch (e) {
      setShareError(`Не удалось отправить заявку: ${String(e)}`);
    } finally {
      setFriendRequestSending(null);
    }
  };

  const handleShareSubmit = async () => {
    if (!shareTarget || !shareFriend || !sharePassword) return;
    setShareSaving(true);
    setShareError(null);
    try {
      const { goodluckShareAccount } = await import("@/lib/tauri");
      const hours = Number(shareDuration);
      await goodluckShareAccount({
        recipientUserId: shareFriend,
        username: shareTarget.Username,
        password: sharePassword,
        riotId: shareTarget.RiotId,
        puuid: shareTarget.Puuid,
        server: shareTarget.Server,
        expiresAt: hours > 0 ? new Date(Date.now() + hours * 3600000).toISOString() : null,
      });
      setSharePassword("");
      setShareTarget(null);
      await fetchSharedAccounts();
      toast.success("Доступ выдан. Другу придёт уведомление — пусть обновит страницу аккаунтов.");
    } catch (e) {
      const message = String(e);
      setShareError(message.includes("totp_required")
        ? "Подтверди 2FA в настройках GoodLuck и повтори выдачу."
        : message);
    } finally {
      setShareSaving(false);
    }
  };

  const handleRevokeShared = async (account: SharedAccount, incoming: boolean) => {
    const text = incoming ? "Отказаться от доступа к этому аккаунту?" : "Отозвать доступ и поставить выход из Riot в очередь на компьютере друга?";
    if (!window.confirm(text)) return;
    try {
      const { goodluckRevokeSharedAccount } = await import("@/lib/tauri");
      await goodluckRevokeSharedAccount(account.id);
      await fetchSharedAccounts();
      toast.success(incoming ? "Доступ закрыт, очистка поставлена в очередь." : "Доступ отозван. Очистка Riot поставлена в очередь.");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleSharedLogin = async (account: SharedAccount) => {
    const key = `shared:${account.id}`;
    setLoginInProgress(key);
    setLoginProgress("");
    setLoginError(null);
    try {
      const { loginToSharedAccount } = await import("@/lib/tauri");
      await loginToSharedAccount(account.id);
    } catch (e) {
      const message = String(e);
      setLoginError(message.includes("totp_required")
        ? "Подтверди 2FA в настройках GoodLuck и повтори вход."
        : `${account.riot_id}: ${message}`);
    } finally {
      setLoginInProgress(null);
      setLoginProgress("");
    }
  };

  const sharedExpiry = (account: SharedAccount) => account.expires_at
    ? `до ${new Date(account.expires_at).toLocaleString("ru-RU", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" })}`
    : "бессрочно";

  return (
    <div className="accounts-page flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-hidden">
      {loginError && (
        <div className="shrink-0 rounded-lg border border-destructive/50 bg-destructive/10 px-4 py-2 text-sm text-destructive">
          {loginError}
        </div>
      )}

      <div className="accounts-header">
        <div className="accounts-header-left">
          <h1 className="shrink-0 text-lg font-semibold tracking-tight">
            Аккаунты
          </h1>
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-2 top-2 size-3.5 text-muted-foreground" />
            <Input
              aria-label="Поиск аккаунтов и друзей"
              placeholder="Поиск"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="h-[30px] pl-7 text-xs"
            />
          </div>
        </div>
        <Tabs
          value={friendsTab ? "friends" : "accounts"}
          onValueChange={(value) => setFriendsTab(value === "friends")}
          className="accounts-tabs"
        >
          <TabsList className="h-[30px] w-full">
            <TabsTrigger value="accounts">Аккаунты</TabsTrigger>
            <TabsTrigger value="friends">Друзья</TabsTrigger>
          </TabsList>
        </Tabs>
        {!friendsTab && (
          <>
            <div className="accounts-header-imports">
              <Button className="accounts-import-button" variant="outline" size="sm" onClick={openGlImportDialog}>
                <Cloud data-icon="inline-start" />
                GoodLuck
              </Button>
              <WithTooltip label="Ник, аватар, ранг из текущей сессии League (клиент должен быть запущен и залогинен)">
                <Button
                  variant="outline"
                  size="sm"
                  className="accounts-import-button"
                  disabled={lcuProfileRefreshing}
                  onClick={async () => {
                    setLcuProfileRefreshing(true);
                    try {
                      const { refreshAccountProfileFromLcu } = await import(
                        "@/lib/tauri"
                      );
                      const { notifyCloudAfterGoodluckSession } = await import(
                        "@/lib/post-login-sync"
                      );
                      const r = await refreshAccountProfileFromLcu();
                      await fetchAccounts();
                      if (r.matchedUsername) {
                        const username = r.matchedUsername;
                        setMatchesError((prev) => {
                          const next = { ...prev };
                          delete next[username];
                          return next;
                        });
                        setMatchesByAccount((prev) => {
                          const next = { ...prev };
                          delete next[username];
                          return next;
                        });
                        setExpandedAccount((current) =>
                          current === username ? null : current,
                        );
                      }
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
                  <RefreshCw
                    className={`h-3.5 w-3.5 mr-1 ${lcuProfileRefreshing ? "animate-spin" : ""}`}
                  />{" "}
                  Из клиента
                </Button>
              </WithTooltip>
              <WithTooltip label="Импорт аккаунтов из файла">
                <Button
                  variant="outline"
                  size="icon-sm"
                  aria-label="Импорт аккаунтов"
                  onClick={handleImport}
                >
                  <Download />
                </Button>
              </WithTooltip>
              <WithTooltip label="Экспорт аккаунтов в файл">
                <Button
                  variant="outline"
                  size="icon-sm"
                  aria-label="Экспорт аккаунтов"
                  onClick={handleExport}
                >
                  <Upload />
                </Button>
              </WithTooltip>
            </div>
            <div className="accounts-header-controls">
              <AutoAcceptSwitch className="accounts-autoaccept" />
              <Button size="sm" onClick={() => setAddOpen(true)}>
                <Plus data-icon="inline-start" />
                Добавить
              </Button>
            </div>
          </>
        )}
      </div>
      {friendsTab ? (
        <LmFriends
          accounts={accounts}
          query={query}
          onShare={(account, friend) => void openShareDialog(account, friend)}
        />
      ) : (
        <>
          {(sharedLoading ||
            sharedAccounts.available_to_me.length > 0 ||
            sharedAccounts.shared_by_me.length > 0) && (
            <section className="shrink-0 overflow-hidden rounded-xl border border-border" aria-label="Доступ к аккаунтам">
              <div className="flex items-center justify-between border-b px-3 py-2">
                <h2 className="text-xs font-medium">Доступ к аккаунтам</h2>
                <WithTooltip label="Обновить доступы"><Button variant="ghost" size="icon-xs" aria-label="Обновить доступы" disabled={sharedLoading} onClick={()=>void fetchSharedAccounts(true)}><RefreshCw className={sharedLoading?'animate-spin':undefined}/></Button></WithTooltip>
              </div>
              <div className="max-h-44 overflow-auto">
              {sharedAccounts.available_to_me.length>0&&<>
                <div className="bg-muted/20 px-3 py-1 text-[10px] text-muted-foreground">Доступно мне · {sharedAccounts.available_to_me.length}</div>
                {sharedAccounts.available_to_me.map(account=><div key={account.id} className="flex items-center justify-between gap-3 border-t px-3 py-2">
                  <div className="min-w-0"><div className="flex items-center gap-2"><span className="truncate text-xs font-medium">{account.riot_id}</span><Badge variant="outline" className="text-[10px]">{account.server}</Badge></div><p className="mt-0.5 truncate text-[11px] text-muted-foreground">От {account.owner_display_name} · {sharedExpiry(account)}</p></div>
                  <div className="flex shrink-0 items-center gap-1">
                    <WithTooltip label={loginInProgress===`shared:${account.id}`?loginProgress||'Вход…':'Войти'}><Button size="icon-sm" aria-label={`Войти: ${account.riot_id}`} disabled={loginInProgress!==null} onClick={()=>void handleSharedLogin(account)}>{loginInProgress===`shared:${account.id}`?<Loader2 className="animate-spin"/>:<LogIn/>}</Button></WithTooltip>
                    <WithTooltip label="Отказаться от доступа и выйти"><Button size="icon-sm" variant="outline" aria-label={`Отказаться от доступа: ${account.riot_id}`} onClick={()=>void handleRevokeShared(account,true)}><ShieldOff/></Button></WithTooltip>
                  </div>
                </div>)}
              </>}
              {sharedAccounts.shared_by_me.length>0&&<>
                <div className="border-t bg-muted/20 px-3 py-1 text-[10px] text-muted-foreground">Выданные мной · {sharedAccounts.shared_by_me.length}</div>
                {sharedAccounts.shared_by_me.map(account=><div key={account.id} className="flex items-center justify-between gap-3 border-t px-3 py-2">
                  <div className="min-w-0"><div className="flex items-center gap-2"><span className="truncate text-xs font-medium">{account.riot_id}</span><Badge variant="outline" className="text-[10px]">{account.server}</Badge><WithTooltip label={`Выдач пароля: ${account.access_count}`}><span className="text-muted-foreground"><Share2 className="size-3"/></span></WithTooltip></div><p className="mt-0.5 truncate text-[11px] text-muted-foreground">Для {account.recipient_display_name} · {sharedExpiry(account)}</p></div>
                  <WithTooltip label="Отозвать доступ"><Button size="icon-sm" variant="destructive" aria-label={`Отозвать доступ: ${account.riot_id}`} onClick={()=>void handleRevokeShared(account,false)}><ShieldOff/></Button></WithTooltip>
                </div>)}
              </>}
              {sharedLoading&&<div role="status" className="flex items-center gap-2 px-3 py-2 text-xs text-muted-foreground"><Loader2 className="size-3 animate-spin"/>Обновляем…</div>}
              </div>
            </section>
          )}

          <div className="accounts-table-shell flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border">
            <div className="accounts-table-scroll min-h-0 min-w-0 flex-1 overflow-auto">
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
                <div>
                  <Table
                    className="min-w-[580px] table-fixed"
                    containerClassName="overflow-visible"
                  >
                    <colgroup>
                      <col style={{ width: "16%" }} />
                      <col style={{ width: "32%" }} />
                      <col style={{ width: "12%" }} />
                      <col style={{ width: "24%" }} />
                      <col style={{ width: "14%" }} />
                    </colgroup>
                    <TableHeader className="sticky top-0 z-10 bg-card">
                      <TableRow>
                        {(
                          [
                            ["Note", "Заметки"],
                            ["RiotId", "Аккаунт"],
                            ["Server", "Сервер"],
                            ["Username", "Логин"],
                          ] as const
                        ).map(([key, label]) => (
                          <TableHead
                            key={key}
                            aria-sort={
                              sort?.key === key
                                ? sort.descending
                                  ? "descending"
                                  : "ascending"
                                : "none"
                            }
                          >
                            <Button
                              variant="ghost"
                              size="xs"
                              className="h-7 px-0"
                              onClick={() => cycleSort(key)}
                            >
                              {label}
                              <ArrowUpDown
                                className={
                                  sort?.key === key
                                    ? "text-primary"
                                    : "opacity-40"
                                }
                              />
                            </Button>
                          </TableHead>
                        ))}
                        <TableHead className="text-right">Действия</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {(() => {
                        const rows: React.ReactNode[] = [];
                        for (const account of visibleAccounts(
                          accounts,
                          query,
                          sort,
                        )) {
                          const isNotActivated = !account.EncryptedPassword;
                          const displayUsername = account.Username.startsWith(
                            "gl:",
                          )
                            ? account.Username.slice(3)
                            : account.Username;
                          rows.push(
                            <TableRow
                              key={account.Username}
                              className={`${isNotActivated ? "opacity-70" : ""} ${expandedAccount === account.Username ? "border-b-0! hover:bg-transparent" : ""}`}
                            >
                              <TableCell className="truncate text-xs text-muted-foreground">
                                <HighlightText text={account.Note || "—"} query={query} />
                              </TableCell>
                              <TableCell>
                                <div className="flex items-center gap-2">
                                  {(account.AvatarUrl ||
                                    account.SummonerLevel > 0) && (
                                    <div className="relative shrink-0">
                                      {account.AvatarUrl ? (
                                        <img
                                          src={account.AvatarUrl}
                                          alt=""
                                          className="size-7 rounded-full"
                                          onError={(e) => {
                                            const img = e.currentTarget;
                                            img.style.display = "none";
                                          }}
                                        />
                                      ) : (
                                        <div className="size-7 rounded-full bg-muted" />
                                      )}
                                      {account.SummonerLevel > 0 && (
                                        <span
                                          className="absolute -bottom-1.5 left-1/2 -translate-x-1/2 rounded-full bg-primary px-1.5 text-[10px] font-semibold leading-4 text-primary-foreground ring-2 ring-background"
                                          aria-label="Уровень призывателя"
                                        >
                                          {account.SummonerLevel}
                                        </span>
                                      )}
                                    </div>
                                  )}
                                  <div className="min-w-0">
                                    <div className="truncate text-xs font-medium">
                                      <HighlightText text={account.RiotId || account.SummonerName || "—"} query={query} />
                                    </div>
                                    {account.RankDisplay && (
                                      <RankBadge display={account.RankDisplay} />
                                    )}
                                  </div>
                                </div>
                              </TableCell>
                              <TableCell className="text-muted-foreground text-sm">
                                <HighlightText text={account.Server || "—"} query={query} />
                              </TableCell>
                              <TableCell className="break-all text-xs text-muted-foreground">
                                <div className="flex flex-wrap items-center gap-1">
                                  <span>
                                    <HighlightText text={hideLogins ? "••••••••" : displayUsername} query={hideLogins ? "" : query} />
                                  </span>
                                  {isNotActivated && (
                                    <Badge
                                      variant="outline"
                                      className="text-xs text-amber-500 border-amber-500/50"
                                    >
                                      Не активирован
                                    </Badge>
                                  )}
                                </div>
                              </TableCell>
                              <TableCell className="text-right">
                                <div className="flex items-center justify-end gap-1">
                                <WithTooltip label={loginInProgress===account.Username?'Отменить вход':isNotActivated?'Активировать':'Войти'}>
                                {loginInProgress===account.Username?<Button size="icon-sm" variant="outline" aria-label="Отменить вход" onClick={async () => {
                                        try {
                                          const { cancelLogin } = await import("@/lib/tauri");
                                          await cancelLogin();
                                        } catch { }
                                      }}><X/></Button>:isNotActivated?<Button size="icon-sm" variant="outline" aria-label="Активировать" disabled={loginInProgress!==null} onClick={()=>openEditDialog(account)}><KeyRound/></Button>:<Button size="icon-sm" aria-label={`Войти: ${account.RiotId||displayUsername}`} disabled={loginInProgress!==null} onClick={async () => {
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
                                    }}><LogIn/></Button>}
                                </WithTooltip>
                                <DropdownMenu>
                                  <DropdownMenuTrigger
                                    render={
                                      <Button
                                        variant="outline"
                                        size="icon-sm"
                                        aria-label={`Действия: ${account.RiotId || displayUsername}`}
                                      />
                                    }
                                  >
                                    <Ellipsis />
                                  </DropdownMenuTrigger>
                                  <DropdownMenuContent
                                    align="end"
                                    className="w-48 accounts-dialog"
                                  >
                                    <DropdownMenuGroup>

                                      <DropdownMenuItem
                                        disabled={loginInProgress !== null}
                                        onClick={() => openEditDialog(account)}
                                      >
                                        <Pencil />
                                        Редактировать
                                      </DropdownMenuItem>
                                      <DropdownMenuItem
                                        disabled={loginInProgress !== null}
                                        onClick={() => handleTogglePin(account)}
                                      >
                                        {account.Pinned ? <PinOff /> : <Pin />}
                                        {account.Pinned
                                          ? "Открепить"
                                          : "Закрепить"}
                                      </DropdownMenuItem>
                                      <DropdownMenuItem
                                        onClick={() =>
                                          void toggleAccountMatches(account)
                                        }
                                      >
                                        <History />
                                        {expandedAccount === account.Username
                                          ? "Скрыть матчи"
                                          : "Последние матчи"}
                                      </DropdownMenuItem>
                                      <DropdownMenuItem
                                        disabled={
                                          isNotActivated ||
                                          loginInProgress !== null
                                        }
                                        onClick={() =>
                                          void openShareDialog(account)
                                        }
                                      >
                                        <Share2 />
                                        Поделиться
                                      </DropdownMenuItem>
                                    </DropdownMenuGroup>
                                    <DropdownMenuSeparator />
                                    <DropdownMenuGroup>
                                      <DropdownMenuItem
                                        variant="destructive"
                                        disabled={loginInProgress !== null}
                                        onClick={() =>
                                          setDeleteTarget(account.Username)
                                        }
                                      >
                                        <Trash2 />
                                        Удалить
                                      </DropdownMenuItem>
                                    </DropdownMenuGroup>
                                  </DropdownMenuContent>
                                </DropdownMenu>
                                </div>
                                {loginInProgress === account.Username && (
                                  <span
                                    role="status"
                                    className="mt-1 flex items-center justify-end gap-1 text-[10px] text-muted-foreground"
                                  >
                                    <Loader2 className="size-3 animate-spin" />
                                    <span className="max-w-24 truncate">
                                      {loginProgress || "Вход…"}
                                    </span>
                                  </span>
                                )}
                              </TableCell>
                            </TableRow>,
                          );

                          if (expandedAccount === account.Username) {
                            rows.push(
                              <TableRow key={`matches-${account.Username}`} className="hover:bg-transparent">
                                <TableCell
                                  colSpan={5}
                                  className="accounts-matches-cell p-2 pt-0"
                                >
                                  <div className="mb-1 flex items-center justify-between">
                                    <span className="text-[11px] font-medium">
                                      Последние 20 матчей
                                    </span>
                                    <WithTooltip label="Свернуть матчи">
                                      <Button
                                        variant="outline"
                                        size="icon-xs"
                                        aria-label="Свернуть матчи"
                                        onClick={() => setExpandedAccount(null)}
                                      >
                                        <ChevronUp />
                                      </Button>
                                    </WithTooltip>
                                  </div>
                                  {matchesLoading === account.Username ? (
                                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                      Загружаю последние матчи...
                                    </div>
                                  ) : matchesError[account.Username] ? (
                                    <p className="text-xs text-destructive">
                                      {matchesError[account.Username]}
                                    </p>
                                  ) : (matchesByAccount[account.Username]
                                      ?.length ?? 0) === 0 ? (
                                    <p className="text-xs text-muted-foreground">
                                      Матчей не нашлось. Проверь подключение
                                      GoodLuck и сервер аккаунта.
                                    </p>
                                  ) : (
                                    <AccountMatches
                                      matches={
                                        matchesByAccount[account.Username] || []
                                      }
                                    />
                                  )}
                                </TableCell>
                              </TableRow>,
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
              <div className="accounts-client-footer flex shrink-0 flex-wrap items-center justify-between gap-2 border-t bg-card px-3 py-2 text-xs">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className={`h-2.5 w-2.5 shrink-0 rounded-full ${
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
          </div>
        </>
      )}

      {/* Password dialog for encrypted export/import */}
      <Dialog open={passwordDialog !== null} onOpenChange={(open) => { if (!open) setPasswordDialog(null); }}>
        <DialogContent className="accounts-dialog sm:max-w-sm">
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
        <DialogContent className="accounts-dialog sm:max-w-md max-h-[calc(100dvh-4rem)] overflow-y-auto max-h-[85vh] flex flex-col gap-0">
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
        <DialogContent className="accounts-dialog sm:max-w-sm">
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
                <SelectContent className="accounts-dialog">
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
        <DialogContent className="accounts-dialog sm:max-w-xs">
          <DialogHeader>
            <DialogTitle>Удалить аккаунт?</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            Аккаунт <span className="font-medium text-foreground">{deleteTarget}</span> будет удалён из LM. Автоимпорт больше не добавит его обратно; вернуть можно вручную.
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

      <Dialog open={shareTarget !== null} onOpenChange={(open) => { if (!open) { setShareTarget(null); setSharePassword(""); } }}>
        <DialogContent className="accounts-dialog sm:max-w-md max-h-[calc(100dvh-4rem)] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Поделиться аккаунтом</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="rounded-md bg-muted/50 p-3 text-sm">
              <div className="font-medium">{shareTarget?.RiotId}</div>
              <div className="text-xs text-muted-foreground">{shareTarget?.Server}</div>
            </div>
            <div className="space-y-2">
              <label className="text-sm text-muted-foreground">Друг GoodLuck</label>
              {friends.length > 0 ? (
                <>
                  <Select value={shareFriend} onValueChange={(value) => value && setShareFriend(value)}>
                    <SelectTrigger className="w-full"><SelectValue placeholder="Выбери друга" /></SelectTrigger>
                    <SelectContent className="accounts-dialog">
                      {friends.map((friend) => <SelectItem key={friend.id} value={friend.id}>{friend.display_name}</SelectItem>)}
                    </SelectContent>
                  </Select>
                  <Button type="button" variant="ghost" size="sm" className="h-7 px-1 text-xs" onClick={() => setFriendSearchOpen((open) => !open)}>
                    <UserPlus className="mr-1 h-3.5 w-3.5" /> Добавить друга
                  </Button>
                </>
              ) : (
                <p className="rounded-md border border-dashed p-3 text-xs text-muted-foreground">
                  Друзей пока нет. Найди пользователя и отправь заявку — открывать пустой список не нужно.
                </p>
              )}
              {friendSearchOpen && (
                <div className="space-y-2 rounded-md border p-3">
                  <div className="flex gap-2">
                    <Input
                      value={friendQuery}
                      onChange={(event) => setFriendQuery(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.preventDefault();
                          void handleFriendSearch();
                        }
                      }}
                      placeholder="Ник или имя на GoodLuck"
                      autoComplete="off"
                    />
                    <Button type="button" variant="outline" onClick={() => void handleFriendSearch()} disabled={friendSearchLoading || friendQuery.trim().length < 2}>
                      {friendSearchLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : "Найти"}
                    </Button>
                  </div>
                  {friendResults.map((user) => (
                    <div key={user.id} className="flex items-center justify-between gap-3 rounded-md bg-muted/50 px-3 py-2 text-sm">
                      <div className="min-w-0">
                        <div className="truncate font-medium">{user.display_name}</div>
                        {user.username && <div className="truncate text-xs text-muted-foreground">@{user.username}</div>}
                      </div>
                      <Button type="button" size="sm" variant="outline" onClick={() => void handleSendFriendRequest(user)} disabled={friendRequestSending !== null}>
                        {friendRequestSending === user.id ? <Loader2 className="h-4 w-4 animate-spin" /> : "Добавить"}
                      </Button>
                    </div>
                  ))}
                </div>
              )}
              {friendNotice && <p className="text-xs text-muted-foreground">{friendNotice}</p>}
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Срок доступа</label>
              <Select value={shareDuration} onValueChange={(value) => value && setShareDuration(value)}>
                <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
                <SelectContent className="accounts-dialog">
                  <SelectItem value="24">24 часа</SelectItem>
                  <SelectItem value="168">7 дней</SelectItem>
                  <SelectItem value="720">30 дней</SelectItem>
                  <SelectItem value="0">Бессрочно</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Пароль Riot-аккаунта</label>
              <Input type="password" value={sharePassword} onChange={(e) => setSharePassword(e.target.value)} placeholder="Только для зашифрованной передачи" autoComplete="off" />
            </div>
            <p className="rounded-md border border-primary/25 bg-primary/5 p-2 text-xs text-muted-foreground">
              Отзыв запрещает новую выдачу и ставит выход из Riot в очередь на устройстве друга. Если его RustLM офлайн, очистка произойдёт при следующей связи; при компрометации также смени пароль Riot. Передача аккаунта может нарушать правила Riot.
            </p>
            {shareError && <p className="text-sm text-destructive">{shareError}</p>}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => { setShareTarget(null); setSharePassword(""); }}>Отмена</Button>
            <Button onClick={() => void handleShareSubmit()} disabled={shareSaving || !shareFriend || !sharePassword}>
              {shareSaving ? <Loader2 className="mr-1 h-4 w-4 animate-spin" /> : <Share2 className="mr-1 h-4 w-4" />}
              {shareSaving ? "Выдача…" : "Поделиться"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit account dialog */}
      <Dialog open={editAccount !== null} onOpenChange={(open) => { if (!open) setEditAccount(null); }}>
        <DialogContent className="accounts-dialog sm:max-w-sm">
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
                <SelectContent className="accounts-dialog">
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
