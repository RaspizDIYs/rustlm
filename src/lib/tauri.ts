import { invoke } from "@tauri-apps/api/core";

export interface AccountRecord {
  Username: string;
  EncryptedPassword: string;
  Note: string;
  CreatedAt: string;
  AvatarUrl: string;
  SummonerName: string;
  Rank: string;
  RankDisplay: string;
  RiotId: string;
  Puuid: string;
  RankIconUrl: string;
  Server: string;
}

// Accounts
export async function loadAccounts(): Promise<AccountRecord[]> {
  return invoke<AccountRecord[]>("load_accounts");
}

export async function saveAccount(account: AccountRecord): Promise<void> {
  return invoke("save_account", { account });
}

export async function deleteAccount(username: string): Promise<void> {
  return invoke("delete_account", { username });
}

export async function saveAccountsOrder(accounts: AccountRecord[]): Promise<void> {
  return invoke("save_accounts_order", { accounts });
}

export async function protectPassword(plain: string): Promise<string> {
  return invoke<string>("protect_password", { plain });
}

export async function exportAccounts(
  path: string,
  password?: string,
  selectedUsernames?: string[]
): Promise<void> {
  return invoke("export_accounts", { path, password, selectedUsernames });
}

export async function importAccounts(
  path: string,
  password?: string
): Promise<number> {
  return invoke<number>("import_accounts", { path, password });
}

// Settings
export async function loadSetting<T>(key: string, defaultValue: T): Promise<T> {
  return invoke<T>("load_setting", { key, defaultValue });
}

export async function saveSetting<T>(key: string, value: T): Promise<void> {
  return invoke("save_setting", { key, value });
}

export async function getAutostartEnabled(): Promise<boolean> {
  return invoke<boolean>("get_autostart_enabled");
}

export async function setAutostartEnabled(enabled: boolean): Promise<void> {
  return invoke("set_autostart_enabled", { enabled });
}

export async function getAutostartBackground(): Promise<boolean> {
  return invoke<boolean>("get_autostart_background");
}

export async function setAutostartBackground(enabled: boolean): Promise<void> {
  return invoke("set_autostart_background", { enabled });
}

export async function shouldStartMinimized(): Promise<boolean> {
  return invoke<boolean>("should_start_minimized");
}

// Logs
export async function getLogLines(): Promise<string[]> {
  return invoke<string[]>("get_log_lines");
}

export async function getLogPath(): Promise<string> {
  return invoke<string>("get_log_path");
}

export async function clearLogs(): Promise<void> {
  return invoke("clear_logs");
}

// Riot Client
export interface ClientConnectivityStatus {
  is_riot_client_running: boolean;
  is_league_running: boolean;
  rc_lockfile_found: boolean;
  lcu_lockfile_found: boolean;
  lcu_port: number | null;
  lcu_http_ok: boolean;
  lcu_lockfile_path: string | null;
  league_install_path: string | null;
}

export interface AccountInfo {
  summoner_name: string;
  avatar_url: string;
  rank: string;
  rank_display: string;
  riot_id: string;
  puuid: string;
  summoner_level: number;
  server: string;
}

export async function isRiotClientRunning(): Promise<boolean> {
  return invoke<boolean>("is_riot_client_running");
}

export async function isLeagueRunning(): Promise<boolean> {
  return invoke<boolean>("is_league_running");
}

export async function killLeague(includeRiotClient: boolean): Promise<void> {
  return invoke("kill_league", { includeRiotClient });
}

export async function restartLeague(): Promise<void> {
  return invoke("restart_league");
}

export async function startRiotClient(): Promise<void> {
  return invoke("start_riot_client");
}

export async function probeConnectivity(): Promise<ClientConnectivityStatus> {
  return invoke<ClientConnectivityStatus>("probe_connectivity");
}

export async function getAccountInfo(): Promise<AccountInfo | null> {
  return invoke<AccountInfo | null>("get_account_info");
}

export interface LcuProfileRefreshResult {
  updated: boolean;
  matchedUsername: string | null;
  message: string;
}

export async function refreshAccountProfileFromLcu(): Promise<LcuProfileRefreshResult> {
  return invoke<LcuProfileRefreshResult>("refresh_account_profile_from_lcu");
}

export async function lcuGet(endpoint: string): Promise<string> {
  return invoke<string>("lcu_get", { endpoint });
}

export async function lcuPost(endpoint: string, body: string): Promise<string> {
  return invoke<string>("lcu_post", { endpoint, body });
}

export async function invalidateLcuCache(): Promise<void> {
  return invoke("invalidate_lcu_cache");
}

export async function detectServer(): Promise<string> {
  return invoke<string>("detect_server");
}

export async function getAuthorizedRiotLoginUsername(): Promise<string> {
  return invoke<string>("get_authorized_riot_login_username");
}

// Data Dragon
export interface ChampionInfo {
  display_name: string;
  english_name: string;
  id: string;
  image_file_name: string;
  tags: string[];
  aliases: string[];
  skins: SkinInfo[];
}

export interface SkinInfo {
  id: number;
  name: string;
  skin_number: number;
  champion_name: string;
  champion_id: number;
  background_skin_id: number;
  splash_url: string;
}

export async function getDdragonVersion(): Promise<string> {
  return invoke<string>("get_ddragon_version");
}

export async function getChampions(): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_champions");
}

export async function getChampionInfo(displayName: string): Promise<ChampionInfo | null> {
  return invoke<ChampionInfo | null>("get_champion_info", { displayName });
}

export async function getSummonerSpells(): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_summoner_spells");
}

export async function getChampionImageUrl(championName: string): Promise<string> {
  return invoke<string>("get_champion_image_url", { championName });
}

// Runes
export interface RuneModel {
  id: number;
  key: string;
  name: string;
  icon: string;
  short_desc: string;
  long_desc: string;
}

export interface RuneSlot {
  runes: RuneModel[];
}

export interface RunePathModel {
  id: number;
  key: string;
  name: string;
  icon: string;
  slots: RuneSlot[];
}

export interface RunePage {
  Name: string;
  PrimaryPathId: number;
  SecondaryPathId: number;
  PrimaryKeystoneId: number;
  PrimarySlot1Id: number;
  PrimarySlot2Id: number;
  PrimarySlot3Id: number;
  SecondarySlot1Id: number;
  SecondarySlot2Id: number;
  SecondarySlot3Id: number;
  StatMod1Id: number;
  StatMod2Id: number;
  StatMod3Id: number;
}

export async function getRunePaths(): Promise<RunePathModel[]> {
  return invoke<RunePathModel[]>("get_rune_paths");
}

export async function getRunePathById(id: number): Promise<RunePathModel | null> {
  return invoke<RunePathModel | null>("get_rune_path_by_id", { id });
}

export async function getRuneById(id: number): Promise<RuneModel | null> {
  return invoke<RuneModel | null>("get_rune_by_id", { id });
}

export async function getStatModsRow1(): Promise<RuneModel[]> {
  return invoke<RuneModel[]>("get_stat_mods_row1");
}

export async function getStatModsRow2(): Promise<RuneModel[]> {
  return invoke<RuneModel[]>("get_stat_mods_row2");
}

export async function getStatModsRow3(): Promise<RuneModel[]> {
  return invoke<RuneModel[]>("get_stat_mods_row3");
}

export async function loadRunePages(): Promise<RunePage[]> {
  return invoke<RunePage[]>("load_rune_pages");
}

export async function saveRunePage(page: RunePage): Promise<void> {
  return invoke("save_rune_page", { page });
}

export async function saveAllRunePages(pages: RunePage[]): Promise<void> {
  return invoke("save_all_rune_pages", { pages });
}

export async function deleteRunePage(pageName: string): Promise<void> {
  return invoke("delete_rune_page", { pageName });
}

// Auto-Accept / Automation
export interface AutomationSettings {
  ChampionToPick1: string;
  ChampionToPick2: string;
  ChampionToPick3: string;
  ChampionToBan: string;
  SummonerSpell1: string;
  SummonerSpell2: string;
  IsEnabled: boolean;
  AutoAcceptMethod: string;
  SelectedRunePageName: string;
  IsPickDelayEnabled: boolean;
  PickDelaySeconds: number;
  AutoRuneGenerationEnabled: boolean;
  PickChampion1?: string | null;
  PickChampion2?: string | null;
  PickChampion3?: string | null;
  BanChampion?: string | null;
  PickChampion1Id?: number | null;
  PickChampion2Id?: number | null;
  PickChampion3Id?: number | null;
  BanChampionId?: number | null;
  Spell1Id?: number | null;
  Spell2Id?: number | null;
  AutoPickEnabled?: boolean;
  AutoBanEnabled?: boolean;
  AutoSpellsEnabled?: boolean;
  AutoRunesEnabled?: boolean;
}

export async function setAutoAcceptEnabled(enabled: boolean): Promise<void> {
  return invoke("set_auto_accept_enabled", { enabled });
}

export async function isAutoAcceptEnabled(): Promise<boolean> {
  return invoke<boolean>("is_auto_accept_enabled");
}

export async function setAutomationSettings(settings: AutomationSettings): Promise<void> {
  return invoke("set_automation_settings", { settings });
}

export async function getAutomationSettings(): Promise<AutomationSettings> {
  return invoke<AutomationSettings>("get_automation_settings");
}

// Login
export async function loginToAccount(username: string): Promise<void> {
  return invoke("login_to_account", { username });
}

export async function cancelLogin(): Promise<void> {
  return invoke("cancel_login");
}

// Customization
export async function setProfileStatus(status: string): Promise<boolean> {
  return invoke<boolean>("set_profile_status", { status });
}

export async function setProfileAvailability(availability: string): Promise<boolean> {
  return invoke<boolean>("set_profile_availability", { availability });
}

export async function setProfileIcon(iconId: number): Promise<boolean> {
  return invoke<boolean>("set_profile_icon", { iconId });
}

export async function setProfileBackground(backgroundSkinId: number): Promise<boolean> {
  return invoke<boolean>("set_profile_background", { backgroundSkinId });
}

export async function getChallenges(): Promise<unknown> {
  return invoke("get_challenges");
}

export async function setChallengeTokens(challengeIds: number[], titleId: number): Promise<boolean> {
  return invoke<boolean>("set_challenge_tokens", { challengeIds, titleId });
}

// Reveal / Spy
export interface PlayerInfo {
  riot_id: string;
  summoner_name: string;
  champion_id: number;
  rank: string;
  tier: string;
  league_points: number;
  wins: number;
  losses: number;
  win_rate: string;
  level: number;
  profile_icon_id: number;
  puuid: string;
  ugg_link: string;
}

export async function getRevealApiConfig(): Promise<[string, string]> {
  return invoke<[string, string]>("get_reveal_api_config");
}

export async function setRevealApiConfig(apiKey: string, region: string): Promise<void> {
  return invoke("set_reveal_api_config", { apiKey, region });
}

export async function testApiKey(): Promise<[boolean, string]> {
  return invoke<[boolean, string]>("test_api_key");
}

export async function getTeamsInfo(): Promise<[PlayerInfo[], PlayerInfo[]]> {
  return invoke<[PlayerInfo[], PlayerInfo[]]>("get_teams_info");
}

export async function sendChatMessage(message: string): Promise<boolean> {
  return invoke<boolean>("send_chat_message", { message });
}

// Updater
export async function checkForUpdate(): Promise<{ available: boolean; version?: string; body?: string }> {
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (update) {
      return { available: true, version: update.version, body: update.body ?? undefined };
    }
    return { available: false };
  } catch {
    return { available: false };
  }
}

export async function installUpdate(): Promise<void> {
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (update) {
    await update.downloadAndInstall();
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  }
}

// Update Settings
export interface UpdateSettingsModel {
  AutoUpdateEnabled: boolean;
  UpdateChannel: string;
  CheckIntervalHours: number;
}

export async function loadUpdateSettings(): Promise<UpdateSettingsModel> {
  return invoke<UpdateSettingsModel>("load_update_settings");
}

export async function saveUpdateSettings(settings: UpdateSettingsModel): Promise<void> {
  return invoke("save_update_settings", { settings });
}

// Migration
export async function checkLolManagerInstalled(): Promise<boolean> {
  return invoke<boolean>("check_lolmanager_installed");
}

export async function uninstallLolManager(): Promise<void> {
  return invoke("uninstall_lolmanager");
}

// Tray
export async function refreshTray(): Promise<void> {
  return invoke("refresh_tray");
}

// GoodLuck Integration
export interface GoodLuckUser {
  user_id: string;
  display_name: string;
  avatar_url: string;
  riot_accounts: GoodLuckRiotAccount[];
  local_avatar_path?: string | null;
}

export interface SyncResult {
  created: number;
  updated: number;
  skipped: number;
}

export async function goodluckLogin(): Promise<void> {
  return invoke("goodluck_login");
}

export async function goodluckHandleCallback(
  code: string,
  callbackState: string
): Promise<GoodLuckUser> {
  return invoke<GoodLuckUser>("goodluck_handle_callback", {
    code,
    callbackState,
  });
}

export async function goodluckGetUser(): Promise<GoodLuckUser | null> {
  return invoke<GoodLuckUser | null>("goodluck_get_user");
}

export async function goodluckRefreshProfile(): Promise<GoodLuckUser> {
  return invoke<GoodLuckUser>("goodluck_refresh_profile");
}

export async function goodluckIsConnected(): Promise<boolean> {
  return invoke<boolean>("goodluck_is_connected");
}

export async function goodluckLogout(): Promise<void> {
  return invoke("goodluck_logout");
}

export async function goodluckSyncAccounts(): Promise<SyncResult> {
  return invoke<SyncResult>("goodluck_sync_accounts");
}

export interface SyncAccountData {
  riot_id: string;
  server: string;
  rank: string;
  summoner_name: string;
}

export async function goodluckDeleteServerData(): Promise<void> {
  return invoke("goodluck_delete_server_data");
}

export async function goodluckGetSyncedAccounts(): Promise<SyncAccountData[]> {
  return invoke<SyncAccountData[]>("goodluck_get_synced_accounts");
}

export interface GoodLuckRiotAccount {
  riot_id: string;
  server: string;
  rank: string;
}

export async function goodluckGetProfileAccounts(): Promise<GoodLuckRiotAccount[]> {
  return invoke<GoodLuckRiotAccount[]>("goodluck_get_profile_accounts");
}

export interface GlImportResult {
  imported: number;
  updated: number;
  skipped: number;
  updated_pairs: [string, string][];
}

export async function goodluckImportProfileAccounts(
  riotAccounts: GoodLuckRiotAccount[]
): Promise<GlImportResult> {
  return invoke<GlImportResult>("goodluck_import_profile_accounts", { riotAccounts });
}

// Greet (test)
export async function greet(name: string): Promise<string> {
  return invoke<string>("greet", { name });
}

// --- Cloud Sync ---

export type SyncStatus =
  | { type: "Idle" }
  | { type: "Syncing" }
  | { type: "Success"; lastSynced: string }
  | { type: "Error"; message: string }
  | { type: "Disconnected" };

export async function cloudSync(): Promise<void> {
  return invoke("cloud_sync");
}

export async function cloudNotifyChange(): Promise<void> {
  return invoke("cloud_notify_change");
}

export async function cloudPush(): Promise<void> {
  return invoke("cloud_push");
}

export async function cloudPull(): Promise<number> {
  return invoke<number>("cloud_pull");
}

export async function cloudGetStatus(): Promise<SyncStatus> {
  return invoke<SyncStatus>("cloud_get_status");
}

export async function cloudTotpSessionActive(): Promise<boolean> {
  return invoke<boolean>("cloud_totp_session_active");
}

export async function cloudDeleteData(): Promise<void> {
  return invoke("cloud_delete_data");
}

// --- TOTP 2FA ---

export interface TotpSetupInfo {
  secret: string;
  otpauthUri: string;
}

export async function totpGetStatus(): Promise<boolean> {
  return invoke<boolean>("totp_get_status");
}

export async function totpSetup(): Promise<TotpSetupInfo> {
  return invoke<TotpSetupInfo>("totp_setup");
}

export async function totpConfirmSetup(code: string): Promise<string[]> {
  return invoke<string[]>("totp_confirm_setup", { code });
}

export async function totpDisable(code: string): Promise<void> {
  return invoke("totp_disable", { code });
}

export async function totpValidate(code: string): Promise<void> {
  return invoke("totp_validate", { code });
}

// --- LoL Config ---

export interface LolConfigStatus {
  path: string | null;
  exists: boolean;
  readonly: boolean;
  league_running: boolean;
}

export interface LolConfigPreset {
  id: string;
  name: string;
  created_at: string;
  source_app_version: string;
}

export async function lolCfgGetStatus(): Promise<LolConfigStatus> {
  return invoke<LolConfigStatus>("lol_cfg_get_status");
}

export async function lolCfgSetReadonly(readonly: boolean): Promise<void> {
  return invoke("lol_cfg_set_readonly", { readonly });
}

export async function lolCfgListPresets(): Promise<LolConfigPreset[]> {
  return invoke<LolConfigPreset[]>("lol_cfg_list_presets");
}

export async function lolCfgCreatePreset(name: string): Promise<LolConfigPreset> {
  return invoke<LolConfigPreset>("lol_cfg_create_preset", { name });
}

export async function lolCfgApplyPreset(id: string): Promise<void> {
  return invoke("lol_cfg_apply_preset", { id });
}

export async function lolCfgDeletePreset(id: string): Promise<void> {
  return invoke("lol_cfg_delete_preset", { id });
}

export async function lolCfgExportPreset(id: string, path: string): Promise<void> {
  return invoke("lol_cfg_export_preset", { id, path });
}

export async function lolCfgImportPreset(path: string): Promise<LolConfigPreset> {
  return invoke<LolConfigPreset>("lol_cfg_import_preset", { path });
}
