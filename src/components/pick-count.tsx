"use client";
import { useEffect, useRef } from "react";
import { Minus, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export function PickCount({ value, onChange }: { value: number; onChange: (count: number) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const current = useRef(value);
  current.current = value;
  useEffect(() => {
    const input = ref.current;
    if (!input) return;
    const wheel = (event: WheelEvent) => {
      if (event.ctrlKey || !event.deltaY) return;
      event.preventDefault();
      current.current = Math.max(1, Math.min(30, current.current + (event.deltaY < 0 ? 1 : -1)));
      onChange(current.current);
    };
    input.addEventListener("wheel", wheel, { passive: false });
    return () => input.removeEventListener("wheel", wheel);
  }, [value, onChange]);
  return <div className="flex items-center gap-1">
    <Button size="icon-sm" variant="outline" aria-label="Уменьшить число пиков" disabled={value <= 1} onClick={() => onChange(value - 1)}><Minus className="size-3" /></Button>
    <Input ref={ref} type="text" inputMode="numeric" role="spinbutton" aria-valuemin={1} aria-valuemax={30} aria-valuenow={value} value={value}
      className="h-7 w-10 px-1 text-center tabular-nums" aria-label="Количество приоритетов ARAM"
      onKeyDown={event => { if (event.key === "ArrowUp" || event.key === "ArrowDown") { event.preventDefault(); onChange(Math.max(1, Math.min(30, value + (event.key === "ArrowUp" ? 1 : -1)))); } }}
      onChange={event => { const count = Number(event.currentTarget.value); if (Number.isInteger(count) && count >= 1 && count <= 30) onChange(count); }} />
    <Button size="icon-sm" variant="outline" aria-label="Увеличить число пиков" disabled={value >= 30} onClick={() => onChange(value + 1)}><Plus className="size-3" /></Button>
  </div>;
}
