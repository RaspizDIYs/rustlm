export function friendsError(error: unknown): string {
 const message=String(error);
 if(/Подтвердите владение|verified|ownership/i.test(message))return "Этот Riot-аккаунт ещё не подтверждён. Подтвердите владение в профиле GoodLuck, затем обновите друзей здесь.";
 if(/Включите обнаружение/i.test(message))return "Включите обнаружение друзей LM.";
 if(/Войдите в League|LCU|lockfile|клиент.*не|не.*клиент/i.test(message))return "Откройте League и войдите в аккаунт: список друзей читается из игрового клиента.";
 if(/401|Unauthorized|not connected|не подключ/i.test(message))return "Войдите в GoodLuck в настройках LM, чтобы загрузить друзей.";
 return "Не удалось обновить друзей. Проверьте подключение и повторите попытку.";
}

export function mergeFriends(
 goodluck: {id:string;username:string;display_name:string;avatar_url?:string|null}[],
 riot: {id:string;username:string;display_name:string;avatar_url?:string|null;riot_accounts?:{puuid:string;riot_id:string}[]}[]
) {
 const merged=new Map<string,(typeof riot)[number]&{sources:("riot"|"goodluck")[]}>();
 for(const friend of goodluck)merged.set(friend.id,{...friend,sources:["goodluck"]});
 for(const friend of riot)merged.set(friend.id,{...merged.get(friend.id),...friend,sources:merged.has(friend.id)?["goodluck","riot"]:["riot"]});
 return [...merged.values()].sort((a,b)=>a.display_name.localeCompare(b.display_name,"ru"));
}
