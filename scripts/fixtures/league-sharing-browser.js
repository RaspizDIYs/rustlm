// Browser-only fixtures. No real Riot credentials, GoodLuck session or network mutations.
(() => {
  const recipient = new URLSearchParams(location.search).get("side") === "recipient";
  const a = "a".repeat(78), b = "b".repeat(78);
  const me = recipient ? b : a, peer = recipient ? a : b;
  const names = { [a]: "GL Shpinat#GULAG", [b]: "Mira#RU2" };
  const account = { Username: recipient ? "mira_fixture" : "shpinat_fixture", EncryptedPassword: "fixture-not-a-password", Note: "Основной", Server: "RU", RiotId: names[me], Puuid: me, AvatarUrl: "", Pinned: false, RankDisplay: recipient ? "GOLD II 40LP" : "EMERALD I 75LP", SummonerLevel: 30 };
  const euwAccount = {...account,Username:"euw_fixture",Server:"EUW",RiotId:"Mira#EUW",Puuid:"e".repeat(78),Note:"EUW"};
  const savedAccounts = [account,euwAccount];
  const storageKey = "rustlm-league-sharing-preview-v1";
  const removedKey=storageKey+":removed";
  // These scenarios use separate storage from sharing and from each other.
  const demo = new URLSearchParams(location.search).get("demo") || "create";
  const inviteCount = Math.max(1, Math.min(8, Number(new URLSearchParams(location.search).get("invitations")) || 1));
  const contactIds = demo === "create" ? [peer, ...Array.from({ length: 9 }, (_, i) => String.fromCharCode(99 + i).repeat(78))] : [peer];
  contactIds.forEach((id, index) => { if (!names[id]) names[id] = `Друг ${index + 1}#DEMO`; });
  const partyKey = storageKey + ":party-v3:" + demo;
  const guestKey = storageKey + ":guest:" + demo + ":" + me;
  const settingsKey = guestKey + ":settings";
  const now = () => new Date().toISOString();
  const member = (id, accepted) => ({ id, name: id === a ? "Shpinat LM" : id === b ? "Mira LM" : names[id].split("#")[0], accepted, ready: false, role: "fill", secondaryRole:"", profileIconId: id === a ? 29 : 7, server: "RU", rankDisplay: id === a ? "EMERALD I 75LP" : "GOLD II 40LP", lobbyCount: 0, lobbyReady: false, gamePuuid: id, gameRiotId: names[id], gameSummonerId: id === a ? "101" : id === b ? "202" : String(300 + contactIds.indexOf(id)), phase: demo === "draft" ? "ChampSelect" : "Lobby", online: true, seenAt: now(), draft: null, draftChoice: null });
  const defaultPreferences = { pick: [{ championId: 99, state: "must" }, { championId: 22, state: "yield" }, { championId: 103, state: "exclude" }], ban: [] };
  const draft = (preferences = defaultPreferences) => {
    const strict = preferences.pick.some(p => p.state === "must");
    return { sessionKey: "424242", mode: "aram", candidates: preferences.pick.filter(p => p.state === (strict ? "must" : "yield") && [22, 99, 103].includes(p.championId)).map(p => p.championId), banCandidates: [], preferences: { pick: preferences.pick, ban: [] } };
  };
  const queues = [{ id: 420, name: "Ранговая Solo/Duo" }, { id: 400, name: "Обычная (драфт)" }, { id: 430, name: "Обычная" }, { id: 440, name: "Ранговая Flex" }, { id: 450, name: "ARAM" }, { id: 2400, name: "ARAM: Хаос" }, { id: 1700, name: "Арена" }];
  const previewSizes = id => id === 420 ? [1, 2] : id === 440 ? [1, 2, 3, 5] : id === 1700 ? [1, 2, 3] : [400,430, 450, 2400].includes(id) ? [1, 2, 3, 4, 5] : [];
  const readParty = () => JSON.parse(localStorage.getItem(partyKey) || "null");
  const listeners = new Map();
  const emitParty = () => { for (const [id, listener] of listeners) if (listener.event === "party-changed") callbacks.get(listener.handler)?.({ event: "party-changed", id, payload: partySnapshot() }); };
  const writeParty = value => { localStorage.setItem(partyKey, JSON.stringify(value)); emitParty(); };
  if (localStorage.getItem(partyKey) === null && ["party", "draft", "roles"].includes(demo)) {
    const members = [member(a, true), member(b, demo === "draft" || demo === "roles")];
    if (demo === "roles") { members[0].role="top";members[0].secondaryRole="middle";members[1].role="jungle";members[1].secondaryRole="utility"; }
    if (demo === "draft") members[1].draft = draft({ pick: [{ championId: 99, state: "yield" }, { championId: 22, state: "yield" }], ban: [] });
    localStorage.setItem(partyKey, JSON.stringify({ id: "preview-party", revision: 1, leaderId: a, queueId: demo === "roles" ? 440 : 450, state: "forming", expiresAt: new Date(Date.now() + 7200000).toISOString(), members, previewLobby: demo === "party" || demo === "roles", previewJoined: demo === "roles" ? [a,b] : demo === "party" ? [a] : [] }));
  }
  const readGuest = () => ({ personal_preset_id: null, ...JSON.parse(localStorage.getItem(guestKey) || JSON.stringify({ active: demo === "guest-auto", changed: false, pending_restore: false, personal_preset_id: demo === "guest-auto" ? "preview-keys" : null, presets: [{ id: "preview-keys", name: "Гостевые клавиши", created_at: now(), source_app_version: "0.5.22" }] })) });
  const writeGuest = value => localStorage.setItem(guestKey, JSON.stringify(value));
  const settings = () => JSON.parse(localStorage.getItem(settingsKey) || "{}");
  const saveSetting = (key, value) => localStorage.setItem(settingsKey, JSON.stringify({ ...settings(), [key]: value }));
  const seedProfile = () => ({Picks:["Lux","Ashe","Ahri"],PickCount:3,Ban:"Garen",AutoPick:true,AutoBan:true,AutoSpells:false,AutoRunes:false,Bench:true,ArenaPick:"preferred",RunePage:""});
  const automation = () => settings().AutomationPreview || {IsEnabled:false,AutoAcceptMethod:"Polling",AutoPickEnabled:true,AutoBanEnabled:true,Profiles:{"classic:top":seedProfile(),aram:{...seedProfile(),Ban:"",AutoBan:false},mayhem:{...seedProfile(),Ban:"",AutoBan:false},arena:seedProfile()}};
  const validDraftContext = (mode, role) => ["aram", "mayhem", "arena"].includes(mode) || mode === "classic" && ["top", "jungle", "middle", "bottom", "utility"].includes(role);
  const draftView = (mode = "aram", role = "") => {
    if (!validDraftContext(mode, role)) throw Error("Unknown preview draft profile");
    const saved = settings().DraftCoordinationPreferences?.[mode === "classic" ? `classic:${role}` : mode] || {pick:[],ban:[]};
    const preferences = {pick:defaultPreferences.pick.map(p=>({...p,state:saved.pick.find(v=>v.championId===p.championId)?.state || "yield"})),ban:["classic","arena"].includes(mode)?[{championId:86,state:saved.ban.find(v=>v.championId===86)?.state || "yield"}]:[]};
    const profile=automation().Profiles?.[mode==="classic"?`classic:${role}`:mode] || {};
    const ids={Lux:99,Ashe:22,Ahri:103,Garen:86};
    preferences.pick=(profile.Picks || []).filter(n=>ids[n]).map(n=>({championId:ids[n],state:saved.pick.find(p=>p.championId===ids[n])?.state || "yield"}));
    preferences.ban=["classic","arena"].includes(mode)?[...new Set((profile.Bans?.length?profile.Bans:[profile.Ban]).filter(n=>ids[n]).map(n=>ids[n]))].slice(0,3).map(id=>({championId:id,state:saved.ban.find(p=>p.championId===id)?.state || "yield"})):[];
    const autoPickEnabled=!!profile.AutoPick,autoBanEnabled=!!profile.AutoBan && ["classic","arena"].includes(mode);
    const coordinationStatus=demo!=="draft"?"solo":!autoPickEnabled&&!autoBanEnabled?"disabled":!(autoPickEnabled&&preferences.pick.length ||autoBanEnabled&&preferences.ban.length)?"empty":"active";
    const live = mode === "aram";
    return { ...draft(preferences), autoPickEnabled, autoBanEnabled, coordinationStatus, sessionKey: live ? "424242" : "", mode, role: mode === "classic" ? role : "", candidates: live ? draft(preferences).candidates : [], preferences, palettePreferences:saved, markedPreferences:{pick:saved.pick.map(p=>p.championId),ban:saved.ban.map(p=>p.championId)}, championNames: { 22: "Ashe", 99: "Lux", 103: "Ahri", 86: "Garen" } };
  };
  const requireParty = (id, accepted = true) => {
    const party = readParty(), own = party?.members.find(m => m.id === me);
    if (!party || party.id !== id || !own || (accepted && !own.accepted)) throw Error("HTTP 403: preview party access denied");
    return { party, own };
  };
  const requireLeader = party => { if (party.leaderId !== me) throw Error("HTTP 403: leader only"); };
  const revision = (party, expected) => { if (party.revision !== expected) throw Error("HTTP 409: revision conflict"); };
  const read = () => JSON.parse(localStorage.getItem(storageKey) || "[]");
  const write = (offers) => { localStorage.setItem(storageKey, JSON.stringify(offers)); };
  const incomingKey = partyKey + ":incoming-seed";
  const initialIncoming = readParty();
  if (recipient && initialIncoming?.members.some(m=>m.id===me && !m.accepted) && !localStorage.getItem(incomingKey)) localStorage.setItem(incomingKey,JSON.stringify(initialIncoming));
  const previewInvitations = party => {
    party = JSON.parse(localStorage.getItem(incomingKey) || "null") || party;
    if (!party || party.members.find(m => m.id === me)?.accepted) return [];
    const hidden = JSON.parse(localStorage.getItem(partyKey + ":declined") || "[]");
    if (!recipient || inviteCount === 1) return hidden.includes(party.id) ? [] : [party];
    return Array.from({length:inviteCount}, (_, index) => {
      const leader = party.members.find(m => m.id === party.leaderId);
      const queueId = [450,430,440,1700][index % 4];
      const id = `demo-leader-${index}`;
      return {...structuredClone(party), id:`preview-invite-${index}`, leaderId:id,queueId,
        members:[{...leader,id,name:index ? `Игрок ${index + 1} LM` : "Shpinat LM",gameRiotId:index ? `Player${index + 1}#RU1` : leader.gameRiotId,profileIconId:7 + index,server:index % 2 ? "EUW" : "RU",lobbyCount:1 + index % (queueId===1700?3:5),lobbyReady:true},structuredClone(party.members.find(m=>m.id===me))], previewLobby:true,previewJoined:[id]};
    }).filter(p=>!hidden.includes(p.id));
  };
  const partySnapshot = () => {
        const party = readParty(), own = party?.members.find(m => m.id === me);
    const current = JSON.parse(localStorage.getItem(guestKey+":current-account") || "null") || (new URLSearchParams(location.search).get("current")==="other" ? {...account,RiotId:"GL Shpinat#GULAG",RankDisplay:"EMERALD I 75LP"} : account);
        if (party) party.members.forEach(m => { m.seenAt = now(); m.server = m.id===me ? current.Server : m.server || "RU"; m.lobbyCount = party.previewLobby ? (party.previewJoined || []).length : 0; m.lobbyReady = m.id === party.leaderId && !!party.previewLobby;m.profileIconId ||= m.id === a ? 29 : 7; if(demo==="draft") {const v=m.id===me?draftView():null;m.automation={mode:"aram",autoPick:v?v.autoPickEnabled:true,autoBan:false,preferences:v?v.preferences:m.draft?.preferences || defaultPreferences};} });
        return { coordinationStatus:settings().DraftCoordinationOptIn===false?"disabled":demo==="draft"?draftView().coordinationStatus:"solo", me, serverNow: now(), active: own?.accepted ? party : null, invitations: previewInvitations(party), queues,
          local: { server:current.Server,profileIconId:current.RiotId.startsWith("GL ")?29:7,rankDisplay:current.RankDisplay, phase: demo === "draft" ? "ChampSelect" : party?.previewLobby ? "Lobby" : "None", riotId: current.RiotId, puuid: current.Puuid, summonerId: recipient ? "202" : "101", lobbyId: party?.previewLobby ? "preview-lobby" : "", lobbyQueueId: party?.previewLobby ? party.queueId : null, lobbyLeader: party?.leaderId === me, lobbyMemberIds: demo==="draft"?party.members.filter(m=>m.accepted).map(m=>m.gameSummonerId):(party?.previewJoined || []).map(id=>party.members.find(m=>m.id===id)?.gameSummonerId).filter(Boolean), matchingInvitation: own?.accepted && party.previewLobby && !party.previewJoined.includes(me) ? "preview-invitation" : null, error: null } };
  };
  const callbacks = new Map(); let sequence = 0;
  window.__previewCalls = [];
  window.__previewSide = recipient ? "recipient" : "sender";
  window.addEventListener("storage", (event) => { if (event.key === storageKey || event.key === removedKey) window.dispatchEvent(new Event("focus")); if (event.key === partyKey) emitParty(); });
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback: (callback) => { callbacks.set(++sequence, callback); return sequence; },
    unregisterCallback: (id) => callbacks.delete(id),
    invoke: async (command, args = {}) => {
      window.__previewCalls.push(command);
      if (command === "plugin:app|version") return "0.5.22";
      if (command === "plugin:event|listen") { const id = ++sequence; listeners.set(id, args); return id; }
      if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return null; }
      if (command === "load_setting") return args.key === "LeagueFriendsVisible" ? true : settings()[args.key] ?? args.defaultValue;
      if (command === "save_setting") { saveSetting(args.key, args.value); return null; }
      if (command === "party_friends") return contactIds.filter(id => id !== peer || !localStorage.getItem(removedKey)).map(id => ({ id: "league:" + id, username: names[id], display_name: id === a ? "Shpinat LM" : id === b ? "Mira LM" : names[id], avatar_url: null }));
      if (command === "party_snapshot") return partySnapshot();
      if (command === "party_create") {
        if (readParty()?.members.some(m => m.id === me && m.accepted)) throw Error("HTTP 409: already in party");
        if (!queues.some(q => q.id === args.queueId) || !Array.isArray(args.invitees) || !previewSizes(args.queueId).includes(args.invitees.length + 1) || args.invitees.some(id => !contactIds.includes(id)) || new Set(args.invitees).size !== args.invitees.length) throw Error("Invalid preview invitees/queue/group size");
        const party = { id: "preview-party", revision: 1, leaderId: me, queueId: args.queueId, state: "forming", expiresAt: new Date(Date.now() + 7200000).toISOString(), members: [member(me, true), ...args.invitees.map(id => member(id, false))], previewLobby: true, previewJoined: [me] };
        writeParty(party); return party;
      }
      if (command === "party_action" && args.request?.action === "decline" && args.partyId?.startsWith("preview-invite-")) {
        const hidden=JSON.parse(localStorage.getItem(partyKey+":declined")||"[]");
        localStorage.setItem(partyKey+":declined",JSON.stringify([...new Set([...hidden,args.partyId])]));return null;
      }
      if (command === "party_join_with_account") {
        const pending=previewInvitations(readParty()).find(p=>p.id===args.partyId);
        if(pending && args.partyId.startsWith("preview-invite-")) writeParty(pending);
        const { party, own } = requireParty(args.partyId, false);
        const leader = party.members.find(m => m.id === party.leaderId);
        const chosen=savedAccounts.find(a=>a.Username===args.username);
        if (!chosen?.EncryptedPassword || chosen.Server !== (leader?.server || "RU")) throw Error("Preview: account server mismatch");
        if (!leader || !party.previewLobby || !party.previewJoined?.includes(party.leaderId)) throw Error("HTTP 409: leader lobby not ready");
        const dismissed=JSON.parse(localStorage.getItem(partyKey+":declined")||"[]");
        localStorage.setItem(partyKey+":declined",JSON.stringify([...new Set([...dismissed,args.partyId])]));
        own.accepted = true;
        own.server = chosen.Server;
        own.gameRiotId = chosen.RiotId;
        own.gamePuuid = chosen.Puuid;
        localStorage.setItem(guestKey+":current-account",JSON.stringify(chosen));
        if (!party.previewJoined.includes(me)) party.previewJoined.push(me);
        party.revision++; writeParty(party); return null;
      }
      if (command === "party_action") {
        const request = args.request, { party, own } = requireParty(args.partyId, false);
        revision(party, request.expectedRevision);
        if (!["join", "decline"].includes(request.action) && !own.accepted) throw Error("HTTP 403: invitation not accepted");
        if (request.action === "join") own.accepted = true;
        else if (request.action === "decline" || request.action === "leave") { if (request.action === "decline" && own.accepted) throw Error("HTTP 409: already accepted"); party.members = party.members.filter(m => m !== own); }
        else if (request.action === "cancel") { requireLeader(party); if (party.state !== "forming") throw Error("HTTP 409: cancel queue first"); writeParty(null); return null; }
        else if (request.action === "update") {
          if (party.state !== "forming") throw Error("HTTP 409: queue active");
          if (request.role !== undefined) { if (!["", "top", "jungle", "middle", "bottom", "utility", "fill"].includes(request.role)) throw Error("Invalid role"); own.role = request.role; }
          if (request.secondaryRole !== undefined) { if (!["", "top", "jungle", "middle", "bottom", "utility"].includes(request.secondaryRole)) throw Error("Invalid secondary role"); own.secondaryRole = request.secondaryRole; }
          if (own.role === "fill") own.secondaryRole="";
          if (request.ready !== undefined) own.ready = !!request.ready;
        } else if (request.action === "draft") { own.draft = request.draft; own.draftChoice = null; }
        else if (request.action === "draftConfirm") {
          const pool = own.draft?.preferences.pick || [], strict = pool.some(p => p.state === "must");
          if (!own.draft?.candidates.includes(request.championId) || !pool.some(p => p.championId === request.championId && p.state === (strict ? "must" : "yield")) || party.members.some(m => m !== own && m.draftChoice === request.championId)) throw Error("HTTP 409: champion unavailable");
          own.draftChoice = request.championId;
        } else throw Error("Unsupported preview party action");
        if (!party.members.some(m => m.id === party.leaderId)) { writeParty(null); return null; }
        party.revision++; writeParty(party); return party;
      }
      if (["party_lobby", "party_accept_lobby", "party_queue", "party_cancel_queue"].includes(command)) {
        const { party } = requireParty(args.partyId);
        if (command === "party_accept_lobby") { if (!party.previewLobby) throw Error("No matching preview invitation"); if (!party.previewJoined.includes(me)) party.previewJoined.push(me); }
        else {
          requireLeader(party);
          if (command === "party_lobby") { if (party.state !== "forming") throw Error("HTTP 409: queue active"); party.previewLobby = true; party.previewJoined = [me]; }
          if (command === "party_queue") { revision(party, args.expectedRevision); if (party.state !== "forming" || !previewSizes(party.queueId).includes(party.members.length) || !party.previewLobby || !party.members.every(m => m.accepted && party.previewJoined.includes(m.id))) throw Error("HTTP 409: members/lobby not ready"); party.state = "queued"; }
          if (command === "party_cancel_queue") party.state = "forming";
        }
        party.revision++; writeParty(party); return null;
      }
      if (command === "draft_local") return draftView(args.mode, args.role);
      if (command === "draft_enabled") return settings().DraftCoordinationOptIn ?? true;
      if (command === "draft_set_enabled") { saveSetting("DraftCoordinationOptIn", !!args.enabled); saveSetting("DraftCoordinationShare", false); return null; }
      if (command === "draft_share_current") {
        if (!(settings().DraftCoordinationEnabled ?? demo === "draft")) throw Error("Preview: enable draft coordination first");
        const { role, championNames, markedPreferences, ...intent } = draftView();
        saveSetting("DraftCoordinationShare", true); return intent;
      }
      if (command === "draft_set_preference") {
        const view = draftView(args.mode, args.role);
        if (!["pick", "ban"].includes(args.kind) || !["must", "yield", "exclude"].includes(args.state)) throw Error("Invalid preview preference");
        if (args.sessionKey && (args.sessionKey !== view.sessionKey || args.mode !== "aram")) throw Error("Preview draft session changed");
        const preference = view.preferences[args.kind].find(p => p.championId === args.championId);
        if (![22,99,103,86].includes(args.championId)) throw Error("Unknown champion");
        if (preference) preference.state = args.state;
        const key = args.mode === "classic" ? `classic:${args.role}` : args.mode;
        const saved=settings().DraftCoordinationPreferences?.[key] || {pick:[],ban:[]};
        saved[args.kind]=[...saved[args.kind].filter(p=>p.championId!==args.championId),{championId:args.championId,state:args.state}];
        saveSetting("DraftCoordinationPreferences", {...settings().DraftCoordinationPreferences,[key]:saved});
        return draftView(args.mode, args.role);
      }
      if (command === "lol_cfg_get_status") return { path: "preview-only/Config", exists: true, readonly: false, league_running: false };
      if (command === "lol_cfg_list_presets") return readGuest().presets;
      if (command === "lol_cfg_guest_status") { const { active, changed, pending_restore, personal_preset_id } = readGuest(); return { active, changed, pending_restore, personal_preset_id }; }
      if (command === "lol_cfg_guest_set_default") {
        const guest = readGuest();
        if (guest.active || !guest.presets.some(p => p.id === args.id)) throw Error("Preview personal preset unavailable");
        guest.personal_preset_id = args.id; writeGuest(guest); return null;
      }
      if (command === "lol_cfg_guest_start") {
        const guest = readGuest();
        if (guest.active || !guest.presets.some(p => p.id === args.id) || !["keyboard", "full"].includes(args.mode ?? "keyboard")) throw Error("Preview guest preset unavailable");
        Object.assign(guest, { active: true, changed: demo === "guest-changed", pending_restore: true }); writeGuest(guest); return null;
      }
      if (command === "lol_cfg_guest_restore") {
        const guest = readGuest();
        if (guest.changed && !args.confirmChanges) throw Error("Preview: confirm changed settings");
        if (args.saveName?.trim()) guest.presets.push({ id: crypto.randomUUID(), name: args.saveName.trim(), created_at: now(), source_app_version: "0.5.22" });
        Object.assign(guest, { active: false, changed: false, pending_restore: false }); writeGuest(guest); return null;
      }
      if (["lol_cfg_", "party_", "draft_"].some(prefix => command.startsWith(prefix))) throw Error("Демонстрация: команда не поддерживается, реальные действия отключены");
      if (command === "load_accounts") return savedAccounts;
      if (command === "goodluck_get_user") return null;
      if (command === "goodluck_get_shared_accounts") return { available_to_me: [], shared_by_me: [] };
      if (command === "get_client_state") return { is_running: true, is_connected: true };
      if (command === "load_update_settings") return { AutoUpdateEnabled: false, CheckIntervalMinutes: 60 };
      if (command === "get_automation_settings") return automation();
      if (command === "set_automation_settings") {saveSetting("AutomationPreview",args.settings);return null;}
      if (command === "league_status") return { connected: true, matches_current: true, riot_id: names[me], owner_riot_id: names[me], puuid: me };
      if (command === "league_refresh") return {
        friends: !localStorage.getItem(removedKey) ? [{ id: "league:" + peer, username: names[peer], display_name: names[peer], accounts: [me], riot_accounts: [{ puuid: peer, riot_id: names[peer] }] }] : [],
        offers: read().filter(o => o.owner_id === me || o.recipient_id === me).map(o => ({ metadata: o.metadata, incoming: o.recipient_id === me })),
      };
      if (command === "league_share") {
        if (args.recipient !== peer || args.username !== account.Username) throw Error("Неверный тестовый получатель");
        const offers = read();
        if(localStorage.getItem(removedKey))throw Error("Контакт удалён из LM");
        if(offers.some(o=>o.owner_id===me && o.recipient_id===peer && o.metadata.riot_id===account.RiotId && Date.parse(o.metadata.expires_at)>Date.now()))throw Error("Этот аккаунт уже предложен другу");
        offers.push({ owner_id: me, recipient_id: peer, metadata: { id: crypto.randomUUID().replaceAll("-", ""), owner: names[me], recipient: names[peer], riot_id: account.RiotId, server: "RU", expires_at: new Date(Date.now() + args.hours * 3600000).toISOString(), status: "pending" } });
        write(offers); return null;
      }
      if (command === "league_remove_friend") { localStorage.setItem(removedKey,"true"); return null; }
      if (command === "league_decide") {
        let offers = read(); const offer = offers.find(o => o.metadata.id === args.id);
        if (!offer) throw Error("Предложение недоступно");
        if (args.action === "accept") {
          if (offer.recipient_id !== me || localStorage.getItem(removedKey)) throw Error("Подтвердить может только получатель");
          offer.metadata.status = "accepted";
        } else {
          if(!["decline","revoke"].includes(args.action) || (args.action==="decline" && offer.recipient_id!==me) || (args.action==="revoke" && offer.owner_id!==me))throw Error("Действие недоступно");
          offers = offers.filter(o => o.metadata.id !== args.id);
        }
        write(offers); return null;
      }
      if (command === "league_login" || command === "login_to_account") throw Error("Демонстрация: реальный вход отключён");
      if (command === "goodluck_get_friends" || command === "goodluck_get_friend_requests" || command === "goodluck_get_synced_accounts") return [];
      if (command === "cloud_get_status") return { type: "Idle" };
      if (command === "get_ddragon_version") return "16.1.1";
      if (command === "get_champion_search_aliases") return { Ahri: ["Ари", "Ahri"], Ashe: ["Эш", "Ashe"], Lux: ["Люкс", "Lux"], Garen: ["Гарен", "Garen"] };
      if (command === "get_champion_info") return {id: String({Lux:99,Ashe:22,Ahri:103,Garen:86}[args.displayName])};
      if (command === "get_champions") return { Ashe: "Ashe", Lux: "Lux", Ahri: "Ahri", Garen:"Garen" };
      if (command === "get_summoner_spells") return {};
      if (["load_rune_pages", "get_rune_paths"].includes(command)) return [];
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
