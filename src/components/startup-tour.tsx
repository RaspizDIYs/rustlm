"use client";
import { useEffect, useState } from "react";
import { usePathname, useRouter } from "next/navigation";
import { ArrowRight, Check, X } from "lucide-react";
import { Popover, PopoverContent } from "@/components/ui/popover";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
import { useSocialPanel } from "@/components/social-panel";
import { TOUR_STEPS, TOUR_STORAGE_KEY, startupTourHidden } from "@/lib/startup-tour";

export function StartupTour() {
  const [step, setStep] = useState<number | null>(null);
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const [hidden, setHidden] = useState(false);
  const [error, setError] = useState("");
  const pathname = usePathname();
  const router = useRouter();
  const { openPanel, closePanel } = useSocialPanel();
  useEffect(() => {
    let suppressed = false;
    try { suppressed = startupTourHidden(localStorage.getItem(TOUR_STORAGE_KEY)); } catch { /* Storage may be disabled by the WebView. */ }
    setHidden(suppressed);
    let timer: ReturnType<typeof setTimeout>;
    const begin = () => {
      if (document.hidden || document.querySelector('[data-slot="dialog-content"]')) { timer = setTimeout(begin, 500); return; }
      setStep(0);
    };
    if (!suppressed) timer = setTimeout(begin, 1200);
    const repeat = () => { clearTimeout(timer); setStep(0); };
    window.addEventListener("rustlm-start-tour", repeat);
    return () => { clearTimeout(timer); window.removeEventListener("rustlm-start-tour", repeat); };
  }, []);
  useEffect(() => {
    setAnchor(null);
    if (step === null) return;
    const current = TOUR_STEPS[step];
    if (pathname.replace(/\/$/, "") !== current.route) { router.push(current.route); return; }
    if ("panel" in current) openPanel(current.panel); else closePanel();
    let target: HTMLElement | null = null;
    let tries = 0;
    let timer: ReturnType<typeof setTimeout>;
    const locate = () => {
      target = document.querySelector<HTMLElement>(`[data-tour="${current.target}"]`);
      if (!target && ++tries < 40) { timer = setTimeout(locate, 100); return; }
      target ??= document.querySelector<HTMLElement>(".lm-sidebar");
      if (current.target.startsWith("repair-")) target?.click();
      target?.scrollIntoView({ block: "nearest", inline: "nearest" });
      if (target) target.dataset.tourActive = "true";
      setAnchor(target);
    };
    locate();
    return () => { clearTimeout(timer); if (target) delete target.dataset.tourActive; };
  }, [step, pathname, router, openPanel, closePanel]);
  const current = step === null ? null : TOUR_STEPS[step];
  const close = () => setStep(null);
  return <Popover open={!!current && !!anchor} onOpenChange={(open, details) => { if (!open) { if (details.reason === "escape-key") close(); else details.cancel(); } }}>
    <PopoverContent anchor={anchor} initialFocus={() => document.getElementById("tour-next")} aria-label="Обзор возможностей LM" aria-labelledby="tour-title" aria-describedby="tour-description">
      {current && <div className="flex flex-col gap-2.5">
        <div className="flex items-center justify-between gap-2"><h2 id="tour-title" className="text-[13px] font-semibold">{current.title}</h2><span className="rounded bg-muted px-1.5 py-0.5 text-[10px] tabular-nums text-muted-foreground">{step! + 1}/{TOUR_STEPS.length}</span></div>
        <p id="tour-description" className="text-xs leading-5 text-muted-foreground">{current.text}</p>
        <label className="flex min-h-6 cursor-pointer items-center gap-2 text-[11px]"><Checkbox aria-label="Не показывать снова" checked={hidden} onCheckedChange={value => {
          try { localStorage.setItem(TOUR_STORAGE_KEY, String(value)); setHidden(value); setError(""); }
          catch { setError("Не удалось сохранить настройку."); }
        }}/>Не показывать снова</label>
        {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
        <div className="flex justify-between gap-2">
          <Button size="sm" className="h-7 px-2 text-[11px]" variant="ghost" onClick={close}><X/>Закрыть</Button>
          <Button id="tour-next" size="sm" className="h-7 px-2.5 text-[11px]" onClick={() => step === TOUR_STEPS.length - 1 ? close() : setStep(step! + 1)}>{step === TOUR_STEPS.length - 1 ? <><Check/>Понятно</> : <>Дальше<ArrowRight/></>}</Button>
        </div>
      </div>}
    </PopoverContent>
  </Popover>;
}
