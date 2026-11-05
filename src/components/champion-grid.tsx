"use client";
import { Check, ArrowLeftRight, Ban } from "lucide-react";
import { memo, type ReactNode } from "react";
import type { ChampionPreference } from "@/lib/party";
import { WithTooltip } from "@/components/ui/with-tooltip";

type PreferenceState = ChampionPreference["state"];
export function ChampionPreferenceMenu({ name, kind, state, change, children }: {
  name: string; kind: "pick" | "ban"; state?: PreferenceState;
  change?: (state?: PreferenceState) => void; children: ReactNode;
}) {
  if (kind === "ban") { state = undefined; change = undefined; }
  const StateIcon = state === "must" ? Check : state === "yield" ? ArrowLeftRight : Ban;
  return <div className="relative rounded-md" data-preference={state} data-pool={kind}
    onContextMenu={change ? event => { event.preventDefault(); event.stopPropagation(); change(); } : undefined}
    onKeyDown={change ? event => { if (event.key === "F10" && event.shiftKey || event.key === "ContextMenu") { event.preventDefault(); event.stopPropagation(); change(); } } : undefined}>
    {children}
    {state && <StateIcon aria-hidden className={`pointer-events-none absolute bottom-0 right-0 z-10 size-3.5 rounded-sm bg-background/90 p-0.5 ${state === "must" ? "text-emerald-500" : state === "yield" ? "text-amber-400" : "text-red-500"}`} />}
    {state && <span aria-hidden className={`pointer-events-none absolute inset-0 rounded-md ring-2 ring-inset ${state === "must" ? "ring-emerald-500" : state === "yield" ? "ring-amber-400" : "ring-red-500"}`} />}
  </div>;
}
export const ChampionGrid = memo(function ChampionGrid({
  entries,
  version,
  selected,
  onSelect,
  kind = "pick",
  preference,
  onPreference,
}: {
  entries: [string, string][];
  version: string;
  selected: string;
  onSelect: (name: string) => void;
  kind?: "pick" | "ban";
  preference?: (name: string) => PreferenceState | undefined;
  onPreference?: (name: string, state?: PreferenceState) => void;
}) {
  return (
    <div
      className="grid max-h-48 grid-cols-[repeat(auto-fill,2.5rem)] justify-start gap-1.5 overflow-y-auto rounded-md border p-2"
      aria-label="Чемпионы"
    >
      {entries.map(([name, english]) => (
        <ChampionPreferenceMenu key={english} name={name} kind={kind} state={preference?.(name)} change={onPreference ? state => onPreference(name, state) : undefined}><WithTooltip label={`${name}${onPreference ? ` · ПКМ: забираю всегда → могу поменяться с тиммейтом → не беру никогда` : ""}`}>
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
        </WithTooltip></ChampionPreferenceMenu>
      ))}
    </div>
  );
});
