export function friendsError(error: unknown): string {
 const message=String(error);
 if(/Подтвердите владение|verified|ownership/i.test(message))return "Этот Riot-аккаунт ещё не подтверждён. Подтвердите владение в профиле GoodLuck, затем обновите друзей здесь.";
 if(/Включите обнаружение/i.test(message))return "Включите обнаружение друзей LM.";
 if(/Войдите в League|LCU|lockfile|клиент.*не|не.*клиент/i.test(message))return "Откройте League и войдите в аккаунт: список друзей читается из игрового клиента.";
 if(/401|Unauthorized|not connected|не подключ/i.test(message))return "Войдите в GoodLuck в настройках LM, чтобы загрузить друзей.";
 return "Не удалось обновить друзей. Проверьте подключение и повторите попытку.";
}
