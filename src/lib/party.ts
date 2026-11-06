import { invoke } from "@tauri-apps/api/core";

export function allowedSizes(queueId: number, maxPlayers?: number | null): readonly number[] | null {
  let sizes: number[] | null = queueId === 420 ? [1,2] : queueId === 440 ? [1,2,3,5]
    : [400,430,480,490,450,2400].includes(queueId) ? [1,2,3,4,5]
    : [1700,1710,1750].includes(queueId) ? [1,2,3] : null;
  if (sizes && typeof maxPlayers === "number" && Number.isInteger(maxPlayers) && maxPlayers > 0) sizes = sizes.filter(size => size <= maxPlayers);
  return sizes;
}

export type PartyRole = "" | "top" | "jungle" | "middle" | "bottom" | "utility" | "fill";
export type ChampionPreference = { championId: number; state: "must" | "yield" | "exclude" };
export type DraftIntent = {
  sessionKey: string; mode: "classic" | "aram" | "mayhem" | "arena";
  candidates: number[]; banCandidates: number[];
  pickOrder?: number | null; pickActive?: boolean; pickRole?: string | null;
  pickContext?: { puuid: string; summonerId: string; pickOrder: number | null; pickActive: boolean; pickRole: string }[];
  preferences: { pick: ChampionPreference[]; ban: ChampionPreference[] };
};
export type PartyMember = {
  id: string; name: string; accepted: boolean; ready: boolean; role: PartyRole; secondaryRole?: PartyRole;
  gamePuuid: string; gameRiotId: string; gameSummonerId: string; phase: string;
  online: boolean; seenAt: string | null; draft: DraftIntent | null; draftChoice: number | null;
  profileIconId?: number; rankDisplay?: string;
  greenPriority?: { championId: number; againstId: string }[];
  server?: string; lobbyCount?: number; lobbyReady?: boolean;
  automation?: {mode: string; autoPick: boolean; autoBan: boolean; preferences: DraftIntent["preferences"]} | null;
};
export type Party = {
  id: string; revision: number; leaderId: string; queueId: number;
  declined?: { id: string; name: string }[];
  state: "forming" | "queuing" | "queued"; expiresAt: string; members: PartyMember[];
};
export type PartyLocal = {
  phase: string; riotId: string; puuid: string; summonerId: string;
  profileIconId?: number; rankDisplay?: string | null; server?: string;
  lobbyId: string; lobbyQueueId: number | null; lobbyLeader: boolean;
  lobbyMemberIds: string[]; matchingInvitation: string | null; error: string | null;
};
export type PartySnapshot = {
  me: string; serverNow: string; active: Party | null; invitations: Party[];
  coordinationStatus?: "solo"|"not-linked"|"disabled"|"empty"|"waiting"|"active";
  local: PartyLocal; queues: { id: number; name: string; maxPlayers?: number | null }[];
};
export type PartyAction = {
  expectedRevision: number;
  action: "join" | "decline" | "leave" | "update" | "cancel" | "beginQueue" | "finishQueue" | "draft" | "draftConfirm";
  role?: PartyRole; secondaryRole?: PartyRole; ready?: boolean; success?: boolean; draft?: DraftIntent; championId?: number;
};
export const partySnapshot = () => invoke<PartySnapshot>("party_snapshot");
export const partyCreate = (queueId: number, invitees: string[]) => invoke<Party>("party_create", { queueId, invitees });
export const partyAction = (partyId: string, request: PartyAction) => invoke<Party | null>("party_action", { partyId, request });
export const partyLobby = (partyId: string) => invoke<void>("party_lobby", { partyId });
export const partyAcceptLobby = (partyId: string) => invoke<void>("party_accept_lobby", { partyId });
export const partyQueue = (partyId: string, expectedRevision: number) => invoke<void>("party_queue", { partyId, expectedRevision });
export const partyCancelQueue = (partyId: string) => invoke<void>("party_cancel_queue", { partyId });

export const partyJoinWithAccount = (partyId: string, username: string) => invoke<void>("party_join_with_account", { partyId, username });
