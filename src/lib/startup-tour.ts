export const TOUR_STORAGE_KEY = "rustlm-hide-startup-tour";
export const TOUR_STEPS = [
  { target: "accounts", route: "/accounts", title: "Ваши аккаунты", text: "Храните несколько Riot-аккаунтов и запускайте League, TFT или Valorant из одного списка." },
  { target: "friends", route: "/accounts", panel: "friends", title: "Друзья справа", text: "Друзья League с LM и GoodLuck собраны здесь — можно передавать им доступ к аккаунтам." },
  { target: "party", route: "/accounts", panel: "party", title: "Собрать пати", text: "Зовите друзей LM: они смогут перезайти на сохранённый аккаунт нужного сервера и присоединиться к лобби." },
  { target: "automation", route: "/automation", title: "Автоматизация", text: "Настройте автопринятие, пики и баны отдельно для режимов и ролей." },
  { target: "champion-pool", route: "/automation", title: "Приоритет чемпионов", text: "ЛКМ выбирает чемпиона, ПКМ меняет отметку: зелёный — беру, жёлтый — могу уступить, красный — не выбирать." },
  { target: "coordination", route: "/automation", title: "Согласование", text: "Включите совместный выбор: LM распределяет желаемых чемпионов и защищает пики друзей от ваших автобанов." },
  { target: "configs", route: "/configs", title: "Ваши настройки", text: "Сохраняйте пресеты; гостевой режим применяет ваши клавиши на переданных аккаунтах с возможностью восстановления." },
  { target: "spy", route: "/spy", title: "Разведка", text: "Смотрите доступные сведения об участниках матча в отдельном компактном списке." },
  { target: "repair-client", route: "/repairs", title: "Если клиент завис", text: "Unglitch перезапускает интерфейс League без перезапуска игры; опасные действия требуют подтверждения." },
  { target: "repair-network", route: "/repairs", title: "Если нет подключения", text: "Repair Tools проверяет сеть и выбранный Zapret, сохраняет резервную копию и позволяет откатить изменения." },
] as const;

export function startupTourHidden(value: string | null): boolean { return value === "true"; }
