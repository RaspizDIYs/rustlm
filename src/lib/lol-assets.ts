// Общие адреса картинок и мелкие форматтеры для страниц, которые показывают
// игроков и матчи (разведка, список аккаунтов).

const CHAMPION_ICON_BASE =
  "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/champion-icons";
const PROFILE_ICON_BASE =
  "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/profile-icons";

export function championIconUrl(championId: number): string | null {
  if (!championId || championId <= 0) return null;
  return `${CHAMPION_ICON_BASE}/${championId}.png`;
}

export function profileIconUrl(iconId: number): string | null {
  if (!iconId || iconId <= 0) return null;
  return `${PROFILE_ICON_BASE}/${iconId}.jpg`;
}

export const POSITION_LABELS: Record<string, string> = {
  top: "Топ",
  jungle: "Лес",
  middle: "Мид",
  bottom: "Бот",
  utility: "Саппорт",
};

export function positionLabel(position: string): string {
  return POSITION_LABELS[position.toLowerCase()] ?? "";
}

/** «5 мин назад», «2 дня назад» — для компактной строки матча. */
export function formatTimeAgo(timestampMs: number): string {
  if (!timestampMs || timestampMs <= 0) return "";
  const diffSec = Math.max(0, Math.floor((Date.now() - timestampMs) / 1000));
  if (diffSec < 60) return "только что";
  const minutes = Math.floor(diffSec / 60);
  if (minutes < 60) return `${minutes} мин назад`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} ч назад`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "вчера";
  return `${days} дн назад`;
}
