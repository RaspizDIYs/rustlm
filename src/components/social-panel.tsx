"use client";

import { createContext, useContext, useEffect, useState, useCallback, useRef } from "react";
import { useRouter } from "next/navigation";
import { Users, UserRoundPlus, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { WithTooltip } from "@/components/ui/with-tooltip";
import { LmFriends } from "@/components/lm-friends";
import { PartyPanel } from "@/components/party-panel";
import { useLeagueSharing } from "@/components/league-sharing";
import type { PartySnapshot } from "@/lib/party";
import { loadAccounts, type AccountRecord, type GoodLuckFriend } from "@/lib/tauri";

type Panel = "friends" | "party";
type ShareRequest = { account: AccountRecord; friend: GoodLuckFriend };
const SocialContext = createContext<{
  panel: Panel | null;
  openPanel: (panel: Panel) => void;
  closePanel: () => void;
  party: PartySnapshot | null;
  setParty: (party: PartySnapshot | null) => void;
  accounts: AccountRecord[];
  setAccounts: (accounts: AccountRecord[]) => void;
  league: ReturnType<typeof useLeagueSharing>;
  shareRequest: ShareRequest | null;
  clearShareRequest: () => void;
  requestShare: (account: AccountRecord, friend: GoodLuckFriend) => void;
} | null>(null);

export function SocialProvider({ children }: { children: React.ReactNode }) {
  const [panel, setPanel] = useState<Panel | null>(null);
  const [accounts, setAccountRecords] = useState<AccountRecord[]>([]);
  const accountRevision = useRef(0);
  const setAccounts = useCallback((value: AccountRecord[]) => { ++accountRevision.current; setAccountRecords(value); }, []);
  const [shareRequest, setShareRequest] = useState<ShareRequest | null>(null);
  const closePanel = useCallback(() => setPanel(null), []);
  const [party, setParty] = useState<PartySnapshot | null>(null);
  const league = useLeagueSharing();
  useEffect(() => {
    let disposed=false;
    let stop: (()=>void)|undefined;
    setParty(null);
    if (!league.status?.connected) return;
    const update=(value:PartySnapshot)=>{ if(!disposed && Array.isArray(value?.invitations)) setParty(value); };
    void import("@tauri-apps/api/event").then(async ({listen})=>{
      const unlisten=await listen<PartySnapshot>("party-changed",event=>update(event.payload));
      if(disposed)unlisten();else stop=unlisten;
    }).catch(()=>{});
    void import("@/lib/party").then(({partySnapshot})=>partySnapshot()).then(update).catch(()=>{});
    return ()=>{disposed=true;stop?.();};
  },[league.status?.connected,league.status?.owner_riot_id]);
  const router = useRouter();
  useEffect(() => {
    let active = true;
    const revision = accountRevision.current;
    void loadAccounts().then(value => { if (active && revision === accountRevision.current) setAccountRecords(value); }).catch(() => {});
    return () => { active = false; };
  }, []);
  return <SocialContext.Provider value={{ panel, openPanel: setPanel, closePanel, party, setParty, accounts, setAccounts, league, shareRequest,
    clearShareRequest: () => setShareRequest(null),
    requestShare: (account, friend) => { setShareRequest({ account, friend }); setPanel(null); router.push("/accounts"); },
  }}>{children}</SocialContext.Provider>;
}

export function useSocialPanel() {
  const value = useContext(SocialContext);
  if (!value) throw new Error("SocialProvider is missing");
  return value;
}

export function SocialPanel() {
  const social = useSocialPanel();
  const [query, setQuery] = useState("");
  const [visited, setVisited] = useState<Panel[]>([]);
  useEffect(() => { if (social.panel) setVisited(old => old.includes(social.panel!) ? old : [...old, social.panel!]); }, [social.panel]);
  const expanded = social.panel !== null;
  return <aside aria-label="Друзья и пати" className={`lm-social-panel flex shrink-0 border-l bg-card ${expanded ? "is-expanded w-[360px]" : "w-[60px]"}`}>
    <div id="social-panel-content" className={expanded ? "flex min-h-0 min-w-0 flex-1 flex-col pt-14" : "hidden"}>
      <div className="social-panel-body flex min-h-0 flex-1 flex-col gap-3 overflow-auto px-3 pb-3 pt-1">
        {(social.panel === "friends" || visited.includes("friends")) && <div className={social.panel === "friends" ? "flex min-h-0 flex-1 flex-col gap-3" : "hidden"}>
          <div className="relative"><Search className="pointer-events-none absolute left-2 top-2 size-3.5 text-muted-foreground"/><Input aria-label="Поиск друзей" placeholder="Поиск друзей" value={query} onChange={event => setQuery(event.target.value)} className="pl-7"/></div>
          <LmFriends accounts={social.accounts} league={social.league} query={query} onShare={social.requestShare}/>
        </div>}
        {(social.panel === "party" || visited.includes("party")) && <div className={social.panel === "party" ? "flex min-h-0 flex-col" : "hidden"}><PartyPanel embedded active={social.panel === "party"}/></div>}
      </div>
    </div>
    <div className="social-panel-rail flex w-[60px] shrink-0 flex-col items-center gap-1 pt-[60px]">
      <WithTooltip label="Друзья"><Button size="icon-sm" variant={social.panel === "friends" ? "secondary" : "ghost"} aria-label="Друзья" data-tour="friends" aria-expanded={social.panel === "friends"} aria-controls="social-panel-content" onClick={() => social.panel === "friends" ? social.closePanel() : social.openPanel("friends")}><Users/></Button></WithTooltip>
      <WithTooltip label="Пати"><Button size="icon-sm" variant={social.panel === "party" ? "secondary" : "ghost"} className="relative" aria-label="Пати" data-tour="party" aria-expanded={social.panel === "party"} aria-controls="social-panel-content" onClick={() => social.panel === "party" ? social.closePanel() : social.openPanel("party")}><UserRoundPlus/>{!!social.party?.invitations.length && <span className="absolute -right-1 -top-1 rounded-full bg-primary px-1 text-[9px] text-primary-foreground">{social.party.invitations.length}</span>}</Button></WithTooltip>
    </div>
  </aside>;
}
