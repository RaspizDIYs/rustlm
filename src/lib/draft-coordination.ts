import type { ChampionPreference, DraftIntent, PartyMember } from "./party";

export type DraftLocal = DraftIntent & { role: string; palettePreferences?: {pick:ChampionPreference[];ban:ChampionPreference[]}; championNames?: Record<string, string>; markedPreferences?: {pick:number[];ban:number[]}; autoPickEnabled?:boolean; autoBanEnabled?:boolean; coordinationStatus?:"solo"|"not-linked"|"disabled"|"empty"|"waiting"|"active" };
export const validChampion = (id: number) => Number.isSafeInteger(id) && id > 0 && id <= 10000;
export const preferenceStates = ["must", "yield", "exclude"] as const;
export const nextPreference = (state: ChampionPreference["state"], marked = true): ChampionPreference["state"] => !marked ? "must" : preferenceStates[(preferenceStates.indexOf(state) + 1) % preferenceStates.length];

function validPool(pool: ChampionPreference[]) {
  return Array.isArray(pool) && pool.length <= 30 && new Set(pool.map(p => p.championId)).size === pool.length
    && pool.every(p => validChampion(p.championId) && preferenceStates.includes(p.state));
}

/** Prefer available greens; use yellows only when no green is available. */
export function allowedDraftCandidates(draft: DraftIntent, kind: "pick" | "ban" = "pick"): number[] {
  const pool = draft.preferences?.[kind];
  const available = kind === "pick" ? draft.candidates : draft.banCandidates;
  if (!pool || !validPool(pool) || !Array.isArray(available) || available.length > 30 || available.some(id => !validChampion(id))) return [];
  const strict = pool.some(p => p.state === "must" && (kind === "ban" || available.includes(p.championId)));
  return pool.filter(p => (strict ? p.state === "must" : p.state === "yield") && available.includes(p.championId)).map(p => p.championId);
}

export function sameDraftIntent(a: DraftIntent | null | undefined, b: DraftIntent | null | undefined): boolean {
  if (!a || !b) return a === b;
  return a.sessionKey === b.sessionKey && a.mode === b.mode && a.pickOrder === b.pickOrder && a.pickActive === b.pickActive && a.pickRole === b.pickRole && JSON.stringify(a.pickContext) === JSON.stringify(b.pickContext)
    && JSON.stringify(a.candidates) === JSON.stringify(b.candidates)
    && JSON.stringify(a.banCandidates) === JSON.stringify(b.banCandidates)
    && (["pick","ban"] as const).every(kind => Array.isArray(a.preferences?.[kind]) && Array.isArray(b.preferences?.[kind])
      && JSON.stringify(a.preferences[kind].map(p => [p.championId,p.state])) === JSON.stringify(b.preferences[kind].map(p => [p.championId,p.state])));
}

/** One user action waits for the current read and takes priority over subsequent polls. */
export class DraftWorkQueue {
  private reading: Promise<void> | null = null;
  mutationPending = false;
  epoch = 0;
  cancel() { this.epoch++; }
  async poll(read: () => Promise<void>) {
    if (this.mutationPending || this.reading) return;
    const task = Promise.resolve().then(read);
    this.reading = task;
    try { await task; } finally { if (this.reading === task) this.reading = null; }
  }
  async run(action: (isCurrent: () => boolean) => Promise<void>) {
    if (this.mutationPending) return false;
    this.mutationPending = true;
    const epoch = this.epoch;
    const isCurrent = () => epoch === this.epoch;
    try {
      // An action revalidates its own data; a failed background read must not drop the click.
      await this.reading?.catch(() => undefined);
      if (!isCurrent()) return false;
      await action(isCurrent);
      return true;
    } finally { this.mutationPending = false; }
  }
}

export function draftIntent(local: DraftLocal): DraftIntent | null {
  if (!/^[1-9]\d*$/.test(local.sessionKey) || !["classic", "aram", "mayhem", "arena"].includes(local.mode)) return null;
  if (!local.preferences || !validPool(local.preferences.pick) || !validPool(local.preferences.ban)) return null;
  if (local.pickOrder != null && (typeof local.pickOrder !== "number" || !Number.isSafeInteger(local.pickOrder) || local.pickOrder < 0 || local.pickOrder > 99)) return null;
  if (local.pickActive !== undefined && typeof local.pickActive !== "boolean") return null;
  if (local.pickRole != null && (typeof local.pickRole !== "string" || !["top","jungle","middle","bottom","utility","unknown"].includes(local.pickRole))) return null;
  if (local.pickContext != null && (!Array.isArray(local.pickContext) || local.pickContext.length > 5 || local.pickContext.some(c =>
    c == null || typeof c !== "object" || typeof c.puuid !== "string" || c.puuid.length > 128 || typeof c.summonerId !== "string" || (c.summonerId !== "" && !/^[1-9]\d{0,19}$/.test(c.summonerId))
    || (c.pickOrder != null && (typeof c.pickOrder !== "number" || !Number.isSafeInteger(c.pickOrder) || c.pickOrder < 0 || c.pickOrder > 99))
    || typeof c.pickActive !== "boolean" || (c.pickActive && c.pickOrder == null)
    || !["top","jungle","middle","bottom","utility","unknown"].includes(c.pickRole)))) return null;
  const noBans = ["aram", "mayhem"].includes(local.mode);
  return { sessionKey: local.sessionKey, mode: local.mode,
    ...(local.mode === "classic" ? {pickOrder:local.pickOrder, pickActive:local.pickActive, pickRole:local.pickRole,pickContext:local.pickContext} : {}), candidates: allowedDraftCandidates(local),
    banCandidates: noBans ? [] : allowedDraftCandidates(local,"ban"),
    preferences: { pick: local.preferences.pick, ban: noBans ? [] : local.preferences.ban } };
}

// At most 5 * 2^5 * 30 states; assignments, greens, shared turns, then list order.
export function coordinateDraft(members: PartyMember[], sessionKey: string, mode: string, actorId?: string): Record<string, number> {
  if (!/^[1-9]\d*$/.test(sessionKey) || members.length > 5 || new Set(members.map(m => m.id)).size !== members.length) return {};
  if (mode === "classic" && actorId !== undefined) {
    const context = members.find(m=>m.id===actorId)?.draft?.pickContext;
    if (!context || context.length>5) return {};
    members = members.filter(m=>context.some(c=>
      (/^[A-Za-z0-9_-]{78}$/.test(c.puuid) ? c.puuid===m.gamePuuid : c.summonerId!=="" && c.summonerId===m.gameSummonerId)
      && c.pickOrder===m.draft?.pickOrder && c.pickActive===m.draft?.pickActive && c.pickRole===m.draft?.pickRole));
  }
  let users = members.filter(m => m.accepted && m.online && m.draft?.sessionKey === sessionKey && m.draft.mode === mode
    && (mode !== "classic" || (m.draft.pickActive === true && Number.isSafeInteger(m.draft.pickOrder) && m.draft.pickOrder! >= 0 && m.draft.pickOrder! <= 99)))
    .sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  if (mode === "classic") {
    const order = Math.min(...users.map(m => m.draft!.pickOrder!));
    users = users.filter(m => m.draft!.pickOrder === order);
  }
  const candidates = users.map(m => allowedDraftCandidates(m.draft!));
  // Confirmed reservations stay fixed; a conflict fails closed.
  const reserved = users.map(m => m.draftChoice).filter((id): id is number => id !== null);
  if (reserved.some((id, i) => !validChampion(id) || reserved.indexOf(id) !== i) || users.some((m, i) => m.draftChoice !== null && !candidates[i].includes(m.draftChoice))) return {};
  const ids = [...new Set(candidates.flat())].sort((a, b) => a - b);
  type Score = [number, number, number, number];
  const better = (a: Score, b: Score): boolean => {
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] > b[i];
    return false;
  };
  type Plan = { score: Score; picks: Record<string, number> };
  let plans = new Map<number, Plan>([[0, { score: [0, 0, 0, 0], picks: {} }]]);
  for (const id of ids) {
    const next = new Map(plans);
    for (const [mask, plan] of plans) for (let i = 0; i < users.length; i++) {
      const user = users[i];
      const owner = users.find(m => m.draftChoice === id);
      if ((mask & (1 << i)) || !candidates[i].includes(id) || (owner && owner !== user) || (user.draftChoice !== null && user.draftChoice !== id)) continue;
      const strict = user.draft!.preferences.pick.some(p => p.championId === id && p.state === "must");
      // Supplied only by the shared server ledger, never by local reservations or hovers.
      const priority = user.greenPriority;
      const fairness = mode !== "classic" && strict && Array.isArray(priority) && priority.length <= 120
        ? users.filter((peer, j) => j !== i && candidates[j].includes(id)
          && peer.draft!.preferences.pick.some(p => p.championId === id && p.state === "must")
          && priority.some(p => p.championId === id && p.againstId === peer.id)).length : 0;
      const score: Score = [plan.score[0] + 1, plan.score[1] + Number(strict),
        plan.score[2] + fairness, plan.score[3] + 30 - candidates[i].indexOf(id)];
      const key = mask | (1 << i);
      if (!next.has(key) || better(score, next.get(key)!.score)) next.set(key, { score, picks: { ...plan.picks, [user.id]: id } });
    }
    plans = next;
  }
  return [...plans.values()].reduce((a, b) => better(b.score, a.score) ? b : a, { score: [-1, 0, 0, 0] as Score, picks: {} }).picks;
}
