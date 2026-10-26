import type { NetworkReport, Pin } from "./network";

function critical(domain: string, port: number, region: string) {
  const server = region === "RU" ? "ru" : region === "EUW" ? "euw1" : "eun1";
  if (domain.endsWith(".chat.si.riotgames.com"))
    return (
      port === 5223 &&
      (region === "ALL" ||
        domain.startsWith(`${region === "RU" ? "ru1" : server}.`))
    );
  if (domain.startsWith("feapp.") && domain.endsWith(".lol.pvp.net"))
    return (
      port === 2099 &&
      (region === "ALL" || domain.startsWith(`feapp.${server}.`))
    );
  if (
    region !== "ALL" &&
    /-red\.lol\.sgp\.pvp\.net$/.test(domain) &&
    !domain.startsWith(`${region.toLowerCase()}-red.`)
  )
    return false;
  if (region !== "ALL" && /^(us|asia)\.edge\.rms\./.test(domain)) return false;
  return (
    port === 443 &&
    /(^|\.)(riotgames\.com|riotcdn\.net|pvp\.net|rgpub\.io|leagueoflegends\.com)$/.test(
      domain,
    )
  );
}
export function repairSignals(report: NetworkReport) {
  const grouped = new Map<string, typeof report.probes>();
  for (const probe of report.probes) {
    if (
      probe.ip.includes(":") ||
      !critical(probe.domain, probe.port, report.context.region)
    )
      continue;
    const key = `${probe.domain}|${probe.port}|${probe.request_path}`;
    grouped.set(key, [...(grouped.get(key) ?? []), probe]);
  }
  const groups = [...grouped.values()];
  return {
    tlsFailures: groups.filter(
      (items) => !items.some((p) => p.tls) && items.some((p) => p.tcp),
    ).length,
    tcpFailures: groups.filter((items) => !items.some((p) => p.tcp)).length,
    cloudflareBlocks: groups.filter((items) =>
      items.every(
        (p) =>
          p.classification === "cloudflare_block" ||
          p.classification === "challenge",
      ),
    ).length,
    testedServices: groups.length,
  };
}
export function dnsRepairPins(report: NetworkReport): Pin[] {
  if (report.cancelled || report.context.connection === "vpn") return [];
  const pins = new Map<string, Pin>();
  for (const probe of report.probes) {
    if (
      !probe.tls ||
      probe.classification !== "http_success" ||
      probe.ip.includes(":") ||
      probe.port !== 443 ||
      !critical(probe.domain, 443, report.context.region)
    )
      continue;
    const system = report.dns.find(
      (d) => d.domain === probe.domain && d.resolver === "system",
    );
    const publicDns = report.dns.some(
      (d) =>
        d.domain === probe.domain &&
        d.resolver !== "system" &&
        d.addresses.includes(probe.ip),
    );
    if (
      system &&
      system.addresses.length === 0 &&
      publicDns &&
      !pins.has(probe.domain)
    )
      pins.set(probe.domain, { domain: probe.domain, ip: probe.ip });
  }
  return [...pins.values()];
}
export function nextRepairVariant(
  current: number,
  report: NetworkReport,
  clientStatus: string,
): number | null {
  if (
    current >= 2 ||
    clientStatus !== "failed" ||
    report.cancelled ||
    report.discovery_limited
  )
    return null;
  return repairSignals(report).tlsFailures > 0 ? current + 1 : null;
}
