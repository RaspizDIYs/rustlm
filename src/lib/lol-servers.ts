export type LolServerOption = { code: string; name: string };

const RAW: LolServerOption[] = [
  { code: "EUW", name: "EU West" },
  { code: "EUNE", name: "EU Nordic & East" },
  { code: "NA", name: "North America" },
  { code: "KR", name: "Korea" },
  { code: "RU", name: "Russia" },
  { code: "TR", name: "Turkey" },
  { code: "BR", name: "Brazil" },
  { code: "JP", name: "Japan" },
  { code: "LAN", name: "Latin America North" },
  { code: "LAS", name: "Latin America South" },
  { code: "OCE", name: "Oceania" },
];

function serverSelectOrder(a: LolServerOption, b: LolServerOption): number {
  const rank = (code: string) => {
    const u = code.toUpperCase();
    if (u === "RU") return 0;
    if (u === "EUW") return 1;
    return 2;
  };
  const ra = rank(a.code);
  const rb = rank(b.code);
  if (ra !== rb) return ra - rb;
  return a.code.localeCompare(b.code, "en");
}

export const LOL_SERVERS_FOR_SELECT = [...RAW].sort(serverSelectOrder);
