const EN_KEYS = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?@#$^&|";
const RU_KEYS = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,\"№;:?/";

export function switchChampionKeyboardLayout(text: string, direction: "ru" | "en"): string {
  const [from, to] = direction === "ru" ? [EN_KEYS, RU_KEYS] : [RU_KEYS, EN_KEYS];
  return Array.from(text, char => {
    const index = from.indexOf(char);
    return index < 0 ? char : to[index];
  }).join("");
}

function fold(text: string): string {
  return text.toLowerCase().replaceAll("ё", "е");
}

function normalize(text: string): string {
  return fold(text).replace(/[^\p{L}\p{N}]/gu, "");
}

export function createChampionSearchIndex(
  champions: Record<string, string>,
  aliases: Record<string, string[]> = {},
) {
  return Object.entries(champions)
    .sort(([a], [b]) => a.localeCompare(b, "ru"))
    .map(([name, key]) => ({
      entry: [name, key] as [string, string],
      aliases: [...new Set([name, key, ...(aliases[key] ?? [])].flatMap(alias => [
        alias,
        switchChampionKeyboardLayout(alias, "ru"),
        switchChampionKeyboardLayout(alias, "en"),
        switchChampionKeyboardLayout(fold(alias), "ru"),
        switchChampionKeyboardLayout(fold(alias), "en"),
      ]).flatMap(alias => [fold(alias), normalize(alias)]))],
    }));
}

export function searchChampions(index: ReturnType<typeof createChampionSearchIndex>, query: string) {
  const bounded = query.slice(0, 256);
  if (!bounded.trim()) return index.map(item => item.entry);
  const variants = [...new Set([fold(bounded.trim()), normalize(bounded)])].filter(Boolean);
  return index.filter(item => variants.some(query => item.aliases.some(alias => alias.includes(query))))
    .map(item => item.entry);
}
