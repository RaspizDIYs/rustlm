"use client";

import { useEffect, useRef } from "react";
import type { MatchSummary } from "@/lib/tauri";
import { championIconUrl } from "@/lib/lol-assets";
import { WithTooltip } from "@/components/ui/with-tooltip";

export function AccountMatches({ matches }: { matches: MatchSummary[] }) {
  const scroll = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = scroll.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      if (event.ctrlKey || element.scrollWidth <= element.clientWidth) return;
      const delta =
        Math.abs(event.deltaX) > Math.abs(event.deltaY)
          ? event.deltaX
          : event.deltaY;
      if (!delta) return;
      event.preventDefault();
      event.stopPropagation();
      element.scrollLeft +=
        delta *
        (event.deltaMode === 1
          ? 24
          : event.deltaMode === 2
            ? element.clientWidth
            : 1);
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, []);

  return (
    <div
      ref={scroll}
      className="accounts-matches-scroll"
      role="region"
      aria-label="Последние матчи — горизонтальная прокрутка"
      tabIndex={0}
      onKeyDown={(event) => {
        const element = event.currentTarget;
        if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
          event.preventDefault();
          element.scrollLeft += event.key === "ArrowRight" ? 73 : -73;
        } else if (event.key === "Home" || event.key === "End") {
          event.preventDefault();
          element.scrollLeft = event.key === "Home" ? 0 : element.scrollWidth;
        }
      }}
    >
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
    </div>
  );
}
