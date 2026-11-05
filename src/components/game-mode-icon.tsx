import { cn } from "@/lib/utils";
import type { AutomationMode } from "@/lib/automation";

export function GameModeIcon({ mode, className }: { mode: AutomationMode | number; className?: string }) {
  const key = typeof mode === "number"
    ? [1700,1710,1750].includes(mode) ? "arena" : [450,2400].includes(mode) ? "aram" : [400,420,430,440,480,490].includes(mode) ? "classic" : null
    : mode;
  const file = key === "arena" ? "nexus-blitz" : key === "aram" || key === "mayhem" ? "howling-abyss" : key === "classic" ? "summoners-rift" : null;
  return file ? <img src={`/images/game-modes/${file}.png`} alt="" aria-hidden="true" draggable={false} className={cn("size-4 shrink-0 object-contain",className)} /> : null;
}
