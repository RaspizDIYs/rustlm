"use client";
import { memo } from "react";
import { WithTooltip } from "@/components/ui/with-tooltip";
export const ChampionGrid = memo(function ChampionGrid({
  entries,
  version,
  selected,
  onSelect,
}: {
  entries: [string, string][];
  version: string;
  selected: string;
  onSelect: (name: string) => void;
}) {
  return (
    <div
      className="grid max-h-48 grid-cols-[repeat(auto-fill,2.5rem)] justify-start gap-1.5 overflow-y-auto rounded-md border p-2"
      aria-label="Чемпионы"
    >
      {entries.map(([name, english]) => (
        <WithTooltip key={english} label={name}>
          <button
            type="button"
            aria-label={name}
            aria-pressed={selected === name}
            data-selected={selected === name || undefined}
            className={`lm-image-choice size-10 rounded-md border-2 ${selected === name ? "border-primary" : "border-transparent"}`}
            onClick={() => onSelect(name)}
          >
            <img
              src={`https://ddragon.leagueoflegends.com/cdn/${version}/img/champion/${english}.png`}
              alt=""
              loading="lazy"
              draggable={false}
              className="size-full"
            />
          </button>
        </WithTooltip>
      ))}
    </div>
  );
});
