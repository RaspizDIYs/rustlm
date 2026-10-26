import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { Installation } from "./network";
export interface ActiveZapret {
  installation: Installation;
  strategy: string | null;
  fingerprint: string;
}
export interface ClientCheck {
  status: "ready" | "failed" | "not_installed" | "login_required";
  message: string;
  login_ready: boolean;
  chat_ready: boolean;
}
export interface EvidenceReport {
  created_at: string;
  errors: { source: string; category: string; count: number }[];
  unavailable: string[];
  path: string;
}
export const detectZapret = () =>
  invoke<ActiveZapret | null>("repair_detect_zapret");
export async function openRepairFolder() {
  await invoke<void>("repair_open_folder");
}
export async function exportRepairReport() {
  const path = await save({defaultPath:`RustLM-report-${Date.now()}.json`,filters:[{name:"Отчёт RustLM",extensions:["json"]}]});
  if (path) await invoke<void>("repair_export_report",{path});
}
export interface RepairReadiness {
  running: boolean;
  strategy_matches: boolean;
  observed_strategy: string | null;
  vpn_signals: string[];
  details_unavailable: boolean;
}
export const stopZapret = (path:string) => invoke<void>("repair_stop_zapret",{path});
export const openVpn = () => invoke<string>("repair_open_vpn");
export const checkRepairReadiness = (path: string, strategy: string) =>
  invoke<RepairReadiness>("repair_readiness", { path, strategy });
export const snapshotZapret = (path: string, strategy: string) =>
  invoke<string>("network_snapshot", { path, strategy });
export const restartZapret = (path: string, strategy: string) =>
  invoke<string>("repair_restart_zapret", { path, strategy });
export const collectRepairEvidence = () =>
  invoke<EvidenceReport>("repair_collect_evidence");
export const verifyRepairClient = () =>
  invoke<ClientCheck>("repair_verify_client");
export const saveRepairJournal = (journal: {id:string;revision:number;step:number;provider:string;region:string;strategy:string;events:string[];report_ids:string[];client_status:string|null;outcome:string|null;operation_failed:boolean}) => invoke<void>("repair_save_journal",{journal});
