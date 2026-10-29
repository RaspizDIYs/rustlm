import type { AccountRecord } from "./tauri";

export type AccountSortKey = "Note" | "RiotId" | "Server" | "Username";
export type AccountSort = { key: AccountSortKey; descending: boolean };
const alphabet = new Intl.Collator(["ru", "en"], {
  sensitivity: "base",
  numeric: true,
});

export function accountLabel(account: AccountRecord): string {
  return (
    account.RiotId ||
    account.SummonerName ||
    account.Username.replace(/^gl:/, "")
  );
}

export function visibleAccounts(
  accounts: AccountRecord[],
  query: string,
  sort: AccountSort | null,
): AccountRecord[] {
  const term = query.trim().toLocaleLowerCase("ru");
  const value = (account: AccountRecord, key: AccountSortKey) =>
    key === "RiotId"
      ? accountLabel(account)
      : key === "Username"
        ? account.Username.replace(/^gl:/, "")
        : account[key];
  return accounts
    .filter(
      (account) =>
        !term ||
        [
          account.Note,
          accountLabel(account),
          account.Server,
          account.Username,
        ].some((text) => text.toLocaleLowerCase("ru").includes(term)),
    )
    .sort((a, b) => {
      if (!sort)
        return (
          Number(b.Pinned) - Number(a.Pinned) ||
          alphabet.compare(accountLabel(a), accountLabel(b))
        );
      const result = alphabet.compare(value(a, sort.key), value(b, sort.key));
      const tie =
        sort.key === "Server"
          ? alphabet.compare(accountLabel(a), accountLabel(b))
          : 0;
      return (result || tie) * (sort.descending ? -1 : 1);
    });
}
