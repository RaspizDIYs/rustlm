export type UnglitchAction = "reload" | "force_reload" | "hide" | "restore";
export interface UnglitchStatus {
  backend_running: boolean;
  ui_running: boolean;
  api_available: boolean;
  phase: string | null;
  force_supported: boolean;
  window_responsive?: boolean | null;
  renderer_count?: number;
  response_ms?: number | null;
  game_running?: boolean;
}

export interface UnglitchWatch {
  enabled: boolean;
  state: string;
  last_result: string | null;
}

export interface ClientDiagnostics {
  ui: { state: string; screen: string | null; elements: number; progress: number;
    disabled_buttons: number; partial: boolean; waiting_ms: number };
  requests: Array<{ area: string; requests: number; errors: number; unavailable: number; last_ms: number; max_ms: number }>;
  events: Array<{ id: number; area: string; outcome: string; duration_ms: number; status: number | null }>;
}

export function diagnosticArea(area: string): string {
  return ({ accept: "Принятие", champions: "Чемпионы", matchmaking: "Поиск", runes: "Руны", loot: "Ремесло", stats: "Статистика" } as Record<string, string>)[area] || "Клиент";
}
export function diagnosticOutcome(outcome: string): string {
  return ({ response: "Ответ получен", slow_response: "Медленный ответ", failed: "Ошибка запроса",
    lcu_confirmed: "Подтверждено LCU", not_confirmed: "Не подтверждено" } as Record<string, string>)[outcome] || "Нет данных";
}
export function diagnosticUiLabel(ui: ClientDiagnostics["ui"] | undefined): string {
  if (!ui) return "Проверяем";
  if (ui.state === "available") return ui.partial ? "Частично доступно" : "Доступно";
  return ({ not_checked: "Нет замера", not_running: "Ждём League", hidden: "Окно скрыто",
    limited: "Мало данных", unavailable: "Нет данных", unsupported: "Не поддерживается",
    simulated: "Тестовый LCU" } as Record<string, string>)[ui.state] || "Нет данных";
}
export async function getClientDiagnostics(capture = false): Promise<ClientDiagnostics> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("unglitch_diagnostics", { capture });
}
export async function exportClientDiagnostics(): Promise<string> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("unglitch_export_diagnostics");
}

export function watchLabel(watch: UnglitchWatch | null): string {
  if (!watch) return "Проверяем";
  if (!watch.enabled) return "Выключено";
  return ({ starting: "Запуск клиента", not_running: "Ждём League", hidden: "Окно скрыто",
    paused: "Пауза", no_connection: "Нет связи", protected: "Без вмешательства",
    healthy: "Работает", observing: "Наблюдаем", suspect: "Проверяем зависание",
    cooldown: "Пауза после попытки", recovering: "Восстанавливаем", recovered: "Восстановлено",
    needs_attention: "Нужна проверка" } as Record<string, string>)[watch.state] || "Наблюдаем";
}

export async function getUnglitchWatch(): Promise<UnglitchWatch> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("unglitch_watch_status");
}

export async function setUnglitchWatch(enabled: boolean): Promise<UnglitchWatch> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("unglitch_watch_enabled", { enabled });
}
export const UNGLITCH_ACTIONS = [
  {
    id: "reload",
    title: "Перезагрузка интерфейса",
    description: "Обновляет окно League встроенной командой клиента.",
    button: "Перезагрузить",
  },
  {
    id: "force_reload",
    title: "Перезапуск рендера",
    description: "Для случаев, когда обычная перезагрузка не помогла.",
    button: "Перезапустить",
  },
  {
    id: "hide",
    title: "Фоновый режим",
    description: "Закрывает интерфейс, оставляя клиент League в фоне.",
    button: "Скрыть окно",
  },
  {
    id: "restore",
    title: "Восстановить интерфейс",
    description: "Возвращает окно фонового клиента League.",
    button: "Открыть окно",
  },
] as const;
export function confirmationRequired(
  action: UnglitchAction,
  phase: string | null,
): boolean {
  if (action === "restore") return false;
  if (action === "hide" || action === "force_reload") return true;
  return !["None", "Lobby", "EndOfGame", "PreEndOfGame"].includes(phase || "");
}
export function actionAvailable(
  action: UnglitchAction,
  status: UnglitchStatus | null,
): boolean {
  if (!status?.backend_running) return false;
  if (action === "restore") return !status.ui_running;
  if (!status.ui_running) return false;
  if (action === "force_reload") return status.force_supported;
  if (action === "hide") return status.api_available;
  return true;
}
export async function getUnglitchStatus(): Promise<UnglitchStatus> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("unglitch_status");
}
export async function executeUnglitch(
  action: UnglitchAction,
  confirmed: boolean,
): Promise<{ message: string; status: UnglitchStatus }> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("unglitch_execute", { action, confirmed });
}
