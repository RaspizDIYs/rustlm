import { serverCode } from "@/lib/lol-servers";
import { Badge } from "@/components/ui/badge";
import { HighlightText } from "@/components/highlight-text";

export function ServerBadge({ server, query = "" }: { server?: string; query?: string }) {
  const region = serverCode(server || "");
  return <Badge variant="outline" className={`server-badge server-${region.toLowerCase().replace(/[^a-z0-9]/g, "") || "unknown"}`}>
    <HighlightText text={region || "—"} query={query}/>
  </Badge>;
}
