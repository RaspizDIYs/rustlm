// Browser-only fixtures. No real Riot credentials, GoodLuck session or network mutations.
(() => {
  const recipient = new URLSearchParams(location.search).get("side") === "recipient";
  const a = "a".repeat(78), b = "b".repeat(78);
  const me = recipient ? b : a, peer = recipient ? a : b;
  const names = { [a]: "GL Shpinat#GULAG", [b]: "Mira#RU2" };
  const account = { Username: recipient ? "mira_fixture" : "shpinat_fixture", EncryptedPassword: "fixture-not-a-password", Note: "Основной", Server: "RU", RiotId: names[me], Puuid: me, AvatarUrl: "", Pinned: false, RankDisplay: "EMERALD I 75LP", SummonerLevel: 30 };
  const storageKey = "rustlm-league-sharing-preview-v1";
  const read = () => JSON.parse(localStorage.getItem(storageKey) || "[]");
  const write = (offers) => { localStorage.setItem(storageKey, JSON.stringify(offers)); };
  const callbacks = new Map(); let sequence = 0;
  window.__previewCalls = [];
  window.__previewSide = recipient ? "recipient" : "sender";
  window.addEventListener("storage", (event) => { if (event.key === storageKey) window.dispatchEvent(new Event("focus")); });
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback: (callback) => { callbacks.set(++sequence, callback); return sequence; },
    unregisterCallback: (id) => callbacks.delete(id),
    invoke: async (command, args = {}) => {
      window.__previewCalls.push(command);
      if (command === "plugin:app|version") return "0.5.22";
      if (command === "plugin:event|listen") return ++sequence;
      if (command === "plugin:event|unlisten") return null;
      if (command === "load_setting") return args.key === "LeagueFriendsVisible" ? true : args.defaultValue;
      if (command === "load_accounts") return [account];
      if (command === "goodluck_get_user") return null;
      if (command === "goodluck_get_shared_accounts") return { available_to_me: [], shared_by_me: [] };
      if (command === "get_client_state") return { is_running: true, is_connected: true };
      if (command === "load_update_settings") return { AutoUpdateEnabled: false, CheckIntervalMinutes: 60 };
      if (command === "get_automation_settings") return { IsEnabled: false, AutoAcceptMethod: "Polling", Profiles: {} };
      if (command === "league_status") return { connected: true, matches_current: true, riot_id: names[me], owner_riot_id: names[me], puuid: me };
      if (command === "league_refresh") return {
        friends: !window.__previewRemoved ? [{ id: "league:" + peer, username: names[peer], display_name: names[peer], accounts: [me], riot_accounts: [{ puuid: peer, riot_id: names[peer] }] }] : [],
        offers: read().filter(o => o.owner_id === me || o.recipient_id === me).map(o => ({ metadata: o.metadata, incoming: o.recipient_id === me })),
      };
      if (command === "league_share") {
        if (args.recipient !== peer || args.username !== account.Username) throw Error("Неверный тестовый получатель");
        const offers = read();
        offers.push({ owner_id: me, recipient_id: peer, metadata: { id: crypto.randomUUID().replaceAll("-", ""), owner: names[me], recipient: names[peer], riot_id: account.RiotId, server: "RU", expires_at: new Date(Date.now() + args.hours * 3600000).toISOString(), status: "pending" } });
        write(offers); return null;
      }
      if (command === "league_remove_friend") { window.__previewRemoved = true; return null; }
      if (command === "league_decide") {
        let offers = read(); const offer = offers.find(o => o.metadata.id === args.id);
        if (!offer) throw Error("Предложение недоступно");
        if (args.action === "accept") {
          if (offer.recipient_id !== me || offer.metadata.status !== "pending") throw Error("Подтвердить может только получатель");
          offer.metadata.status = "accepted";
        } else offers = offers.filter(o => o.metadata.id !== args.id);
        write(offers); return null;
      }
      if (command === "league_login" || command === "login_to_account") throw Error("Демонстрация: реальный вход отключён");
      if (command === "goodluck_get_friends" || command === "goodluck_get_friend_requests" || command === "goodluck_get_synced_accounts") return [];
      if (command === "cloud_get_status") return { type: "Idle" };
      if (command === "get_ddragon_version") return "16.1.1";
      if (command === "get_champions") return {};
      return null;
    },
  };
  window.addEventListener("DOMContentLoaded", () => {
    const label = document.createElement("div");
    label.textContent = `Демонстрация · ${recipient ? "Получатель — Mira" : "Отправитель — Shpinat"} · GoodLuck отключён · реальные передачи отключены`;
    label.style.cssText = "position:fixed;bottom:0;left:0;right:0;padding:5px 12px;background:#12213e;color:#b8cdfb;font:11px system-ui;z-index:9999;text-align:center;pointer-events:none";
    document.body.append(label);
  });
})();
