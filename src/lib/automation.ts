import type { AutomationProfile, AutomationSettings } from "./tauri";
export const MODES = {
  classic: "Обычные / ранговые",
  aram: "ARAM",
  mayhem: "ARAM: Хаос",
  arena: "Арена",
};
export const ROLES = {
  top: "Топ",
  jungle: "Лес",
  middle: "Мид",
  bottom: "ADC",
  utility: "Поддержка",
};
export type AutomationMode = keyof typeof MODES;
export function editorProfile(
  settings: AutomationSettings,
  key: string,
): AutomationProfile {
  if (settings.Profiles?.[key]) return settings.Profiles[key];
  if (key === "classic")
    return {
      Picks: [
        settings.PickChampion1 || "",
        settings.PickChampion2 || "",
        settings.PickChampion3 || "",
      ],
      Ban: settings.BanChampion || "",
      Spell1Id: settings.Spell1Id,
      Spell2Id: settings.Spell2Id,
      RunePage: settings.SelectedRunePageName || "",
      AutoPick: settings.AutoPickEnabled ?? true,
      AutoBan: settings.AutoBanEnabled ?? true,
      AutoSpells: settings.AutoSpellsEnabled ?? true,
      AutoRunes: settings.AutoRunesEnabled ?? true,
      Bench: false,
      ArenaPick: "preferred",
    };
  return {
    Picks: ["", "", ""],
    Ban: "",
    RunePage: "",
    AutoPick: false,
    AutoBan: false,
    AutoSpells: false,
    AutoRunes: false,
    Bench: false,
    ArenaPick: "preferred",
  };
}
export function allowedSpells(mode: AutomationMode): number[] {
  return mode === "arena"
    ? []
    : mode === "classic"
      ? [1, 3, 4, 6, 7, 11, 12, 14, 21]
      : mode === "aram"
        ? [1, 3, 4, 6, 7, 13, 14, 21, 32]
        : [1, 4, 6, 7, 13, 14, 21, 32];
}

export function pickCount(
  profile: AutomationProfile,
  mode: AutomationMode,
): number {
  return Math.max(
    1,
    Math.min(
      mode === "aram" || mode === "mayhem" ? 30 : 3,
      profile.PickCount ?? Math.max(3, profile.Picks.length),
    ),
  );
}
export function jungleSpells(profile: AutomationProfile): AutomationProfile {
  if (profile.Spell2Id === 11 && profile.Spell1Id !== 11) return profile;
  const other =
    profile.Spell2Id != null && profile.Spell2Id !== 11
      ? profile.Spell2Id
      : profile.Spell1Id != null && profile.Spell1Id !== 11
        ? profile.Spell1Id
        : null;
  return { ...profile, Spell1Id: 11, Spell2Id: other };
}
