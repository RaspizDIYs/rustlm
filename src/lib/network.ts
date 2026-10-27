import { Channel, invoke } from "@tauri-apps/api/core";

export type Connection = "direct" | "vpn" | "zapret";
export interface RunContext {
  connection: Connection;
  provider: string;
  region: "RU" | "EUW" | "EUNE" | "ALL";
  strategy: string | null;
  strategy_hash: string | null;
  observed_strategy?: string | null;
  observed_command_hash?: string | null;
  network_fingerprint?: string | null;
}
export interface DnsResult {
  domain: string;
  resolver: string;
  addresses: string[];
  aliases: string[];
  error: string | null;
}
export interface Probe {
  domain: string;
  ip: string;
  port: number;
  tcp: boolean;
  tls: boolean;
  http: number | null;
  classification: string;
  server: string | null;
  ray: string | null;
  title: string | null;
  error: string | null;
  elapsed_ms: number;
  request_path: string;
}
export interface NetworkReport {
  id: string;
  created_at: string;
  context: RunContext;
  dns: DnsResult[];
  probes: Probe[];
  service_domains?: string[];
  cancelled: boolean;
  path: string;
  discovery_limited: boolean;
}
export interface Progress {
  phase: string;
  completed: number;
  total: number;
  current: string;
}
export interface Installation {
  root: string;
  strategies: string[];
  backups: string[];
  backup_directory: string;
  profiles: {strategy: string; variant: number; state: "original" | "configured" | "modified"}[];
}
export interface Patch {
  root: string;
  strategy: string;
  before_hash: string;
  before: string;
  after: string;
  domains: string;
  variant: number;
  source_report: string | null;
  legacy_ips: string | null;
}
export interface Pin {
  domain: string;
  ip: string;
}
export interface HostsPlan {
  report_id: string;
  before_hash: string;
  after: string;
  pins: Pin[];
}
export const previewHosts = (id: string, pins: Pin[]) =>
  invoke<HostsPlan>("network_hosts_preview", { id, pins });
export const applyHosts = (plan: HostsPlan) =>
  invoke<string>("network_hosts_apply", { plan });
export const restoreHosts = (backupId: string) =>
  invoke<void>("network_hosts_restore", { backupId });

export const inspectZapret = (path: string) =>
  invoke<Installation>("network_inspect", { path });
export const loadNetworkHistory = () =>
  invoke<NetworkReport[]>("network_history");
export const previewNetworkPatch = (
  path: string,
  strategy: string,
  variant = 0,
  reportId: string | null = null,
) => invoke<Patch>("network_preview", { path, strategy, variant, reportId });
export const applyNetworkPatch = (patch: Patch) =>
  invoke<string>("network_apply", { patch });
export const restoreNetworkPatch = (path: string, backupId: string) =>
  invoke<void>("network_restore", { path, backupId });
export const selectNetworkProfile = (path: string, strategy: string, configured: boolean) =>
  invoke<string>("network_select_profile", { path, strategy, configured });
export const cancelNetworkRun = () => invoke<void>("network_cancel");
export function runNetwork(
  context: RunContext,
  installation: string | null,
  progress: (p: Progress) => void,
) {
  const channel = new Channel<Progress>();
  channel.onmessage = progress;
  return invoke<NetworkReport>("network_run", {
    context,
    installation,
    onProgress: channel,
  });
}

export function compareReports(before: NetworkReport, after: NetworkReport) {
  const keyed = new Map(
    before.probes.map((p) => [
      `${p.domain}|${p.ip}|${p.port}|${p.request_path}`,
      p,
    ]),
  );
  const changes = after.probes.flatMap((p) => {
    const previous = keyed.get(
      `${p.domain}|${p.ip}|${p.port}|${p.request_path}`,
    );
    return previous ? [{ previous, current: p }] : [];
  });
  return {
    shared: changes.length,
    tlsGained: changes.filter(
      ({ previous, current }) => !previous.tls && current.tls,
    ).length,
    tlsLost: changes.filter(
      ({ previous, current }) => previous.tls && !current.tls,
    ).length,
    blocksBefore: changes.filter(
      ({ previous }) => previous.classification === "cloudflare_block",
    ).length,
    blocksAfter: changes.filter(
      ({ current }) => current.classification === "cloudflare_block",
    ).length,
    sameContext:
      before.context.provider === after.context.provider &&
      before.context.region === after.context.region &&
      !!before.context.network_fingerprint &&
      before.context.network_fingerprint === after.context.network_fingerprint,
  };
}

export const classificationLabels: Record<string, string> = {
  tcp_error: "Нет TCP-соединения",
  tls_error: "Ошибка TLS",
  tls_success: "TLS установлен",
  cloudflare_block: "Блокировка Cloudflare",
  challenge: "Проверка Cloudflare",
  authentication_required: "Нужна авторизация",
  http_forbidden: "HTTP 403: отказ",
  http_success: "Успешный HTTP-ответ",
  http_response: "HTTP-ответ",
  http_error: "Ошибка HTTP",
};
