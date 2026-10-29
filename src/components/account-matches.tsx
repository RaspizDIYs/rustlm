"use client";

import { HorizontalScroll } from "@/components/horizontal-scroll";
import type { MatchSummary } from "@/lib/tauri";
import { championIconUrl } from "@/lib/lol-assets";
import { WithTooltip } from "@/components/ui/with-tooltip";

export function AccountMatches({ matches }: { matches: MatchSummary[] }) {
  return (
    <HorizontalScroll label="Последние матчи — горизонтальная прокрутка">
      <div className="flex w-max gap-1" role="list">
        {matches.slice(0, 20).map((match) => (
          <WithTooltip
            key={match.match_id}
            label={`${match.champion_name} · ${match.win ? "Победа" : "Поражение"}`}
          >
            <div
              role="listitem"
              className={`accounts-match ${match.win ? "accounts-match-win" : "accounts-match-loss"}`}
              aria-label={`${match.champion_name}, ${match.win ? "победа" : "поражение"}, ${match.kills}/${match.deaths}/${match.assists}`}
            >
              <img
                src={championIconUrl(match.champion_id) || undefined}
                alt={match.champion_name}
                className="size-6 rounded object-cover"
                loading="lazy"
              />
              <span className="flex items-center gap-1 whitespace-nowrap font-sans text-[11px] font-medium tabular-nums leading-none">
                <span className={match.win ? "text-green-500" : "text-red-500"}>
                  {match.win ? "W" : "L"}
                </span>
                <span>
                  {match.kills}/{match.deaths}/{match.assists}
                </span>
              </span>
            </div>
          </WithTooltip>
        ))}
      </div>
    </HorizontalScroll>
  );
}
