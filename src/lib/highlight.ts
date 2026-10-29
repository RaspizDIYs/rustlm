export function highlightParts(
  text: string,
  query: string,
): { text: string; match: boolean }[] {
  const term = query.trim();
  if (!term) return [{ text, match: false }];
  const escaped = [...term]
    .map((char) => ("\\^$.*+?()[]{}|".includes(char) ? "\\" + char : char))
    .join("");
  return text
    .split(new RegExp(`(${escaped})`, "giu"))
    .map((part, index) => ({ text: part, match: index % 2 === 1 }));
}
