import type { PartyRole } from "./party";
const positions = ["top", "jungle", "middle", "bottom", "utility"];
export function partyRoleIssue(members: { role: PartyRole; secondaryRole?: PartyRole }[]): string | null {
  if (!members.length) return "Нет участников";
  const pairs = new Set<string>();
  const covered = new Set<string>();
  for (const member of members) {
    if (member.role === "fill") { positions.forEach(role => covered.add(role)); continue; }
    if (!positions.includes(member.role) || !member.secondaryRole || !positions.includes(member.secondaryRole) || member.role === member.secondaryRole) return "Выберите основную и вторую роли";
    const pair = [member.role, member.secondaryRole].sort().join(":");
    if (pairs.has(pair)) return "Участникам нужны разные пары ролей";
    pairs.add(pair); covered.add(member.role); covered.add(member.secondaryRole);
  }
  const minimum = Math.min(5, members.length + 1);
  return covered.size < minimum ? `Разных ролей: ${covered.size}/${minimum}` : null;
}
export function choosePartyRole(primary: PartyRole, secondary: PartyRole | undefined, next: PartyRole, position: 1 | 2) {
  if (position === 1) {
    if (next === "fill") return { role: next, secondaryRole: "" as PartyRole };
    return { role: next, secondaryRole: next === secondary ? primary === "fill" ? "" as PartyRole : primary : secondary || "" as PartyRole };
  }
  return { role: primary, secondaryRole: next };
}
