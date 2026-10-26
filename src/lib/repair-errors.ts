export function repairError(raw: string) {
  let title = "Не удалось завершить операцию";
  let action = "Повторите попытку. Если ошибка повторяется, сохраните технические подробности.";
  if (/ZAPRET_DETAILS_UNAVAILABLE/.test(raw)) {
    title = "Zapret запущен с правами администратора";
    action = "Папку повторно выбирать не нужно. Нажмите «Включить» или начните настройку: LM подтвердит выбранную стратегию через помощник Windows.";
  } else if (/VPN_DETECTED/.test(raw)) {
    title = "Обнаружены признаки VPN или прокси";
    action = "Отключите VPN и прокси, включите привычную стратегию Zapret и повторите проверку состояния.";
  } else if (/ZAPRET_NOT_RUNNING/.test(raw)) {
    title = "Zapret не запущен";
    action = "Включите выбранную стратегию Zapret и повторите проверку состояния. Замеры и настройка не начаты.";
  } else if (/STRATEGY_MISMATCH/.test(raw)) {
    title = "Запущенная стратегия отличается от выбранной";
    action = "Выберите действующий BAT-файл или запустите нужную стратегию, затем проверьте состояние ещё раз.";
  } else if (/UAC_CANCELLED|1223/.test(raw)) {
    title = "Запрос прав администратора отменён";
    action = "Повторите операцию и подтвердите запрос Windows, если хотите продолжить.";
  } else if (/SERVICE_MODE/.test(raw)) {
    title = "Zapret работает как служба";
    action = "Автоматический перезапуск службы пока не поддерживается. Запустите привычную стратегию через BAT-файл.";
  } else if (/VALIDATION_FAILED/.test(raw)) {
    title = "Не удалось проверить параметры Zapret";
    action = /RESTART_NOT_STARTED/.test(raw)
      ? "Работающий Zapret не останавливался. Проверьте выбранную стратегию и технические подробности."
      : "Проверьте выбранную стратегию и технические подробности.";
  } else if (/RESTART_FAILED_AFTER_STOP/.test(raw)) {
    title = "Zapret не запустился с новыми параметрами";
    action = "Проверьте результат отката. Если он не завершён, нажмите «Откатить изменения».";
  } else if (/CLIXML|<Objs|Операция администратора не завершена/.test(raw)) {
    title = "Windows не завершила операцию с правами администратора";
    action = "Проверьте результат отката и повторите попытку. Служебный вывод доступен в подробностях.";
  } else if (raw.length < 400 && !/[<>]|At line:|CategoryInfo/.test(raw)) {
    title = raw.replace(/^Error:\s*/, "");
  }
  const rollbackFailed = /Откат не завершён/.test(raw);
  return { title, action: rollbackFailed ? `${action} Откат не завершён: резервная копия сохранена, воспользуйтесь кнопкой отката.` : action };
}
