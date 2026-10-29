export function logLevel(line: string): string | undefined {
  return line.match(/\[(LOGIN|HTTP|UI|PROC|INFO|WARN|ERROR|DEBUG|TRACE)\]/)?.[1];
}
export function appendLogLines(previous: string[], incoming: string[], reset: boolean): string[] {
  if (!reset && incoming.length === 0) return previous;
  return (reset ? incoming : [...previous, ...incoming]).slice(-2000);
}
