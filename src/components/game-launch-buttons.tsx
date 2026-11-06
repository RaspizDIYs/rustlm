"use client";
import { useRef } from "react";
import { Button } from "@/components/ui/button";
import { WithTooltip } from "@/components/ui/with-tooltip";
export type RiotGame = "league" | "tft" | "valorant";
const games = [
  { id: "league", name: "League of Legends", icon: "leagueoflegends.svg" },
  { id: "tft", name: "TFT", icon: "tft.svg" },
  { id: "valorant", name: "Valorant", icon: "valorant.svg" },
] as const;
export function GameLaunchButtons({ disabled, onLaunch }: { disabled: boolean; onLaunch: (game: RiotGame) => Promise<void> }) {
  const pending = useRef(false);
  return <div className="flex shrink-0 items-center gap-1">{games.map(game => <WithTooltip key={game.id} label={`Войти в ${game.name}`}>
    <Button size="icon-sm" variant="outline" disabled={disabled} aria-label={`Войти в ${game.name}`} onClick={() => {
      if (pending.current) return;
      pending.current = true;
      void onLaunch(game.id).finally(() => { pending.current = false; });
    }}>
      <img src={`/games/${game.icon}`} alt="" className="size-4 object-contain" />
    </Button>
  </WithTooltip>)}</div>;
}
