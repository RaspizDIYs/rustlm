export type UnglitchAction = "reload" | "force_reload" | "hide" | "restore";
export interface UnglitchStatus {
  backend_running: boolean;
  ui_running: boolean;
  api_available: boolean;
  phase: string | null;
  force_supported: boolean;
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
