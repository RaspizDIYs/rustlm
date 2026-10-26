"use client";
import { CircleAlert } from "lucide-react";
import { repairError } from "@/lib/repair-errors";

export function RepairError({ error }: { error: string }) {
  const message = repairError(error);
  return <div className="min-w-0 rounded-xl border border-destructive/30 bg-destructive/10 p-4">
    <div role="alert" className="flex items-start gap-3">
      <CircleAlert aria-hidden className="mt-0.5 size-5 shrink-0 text-destructive" />
      <div><p className="font-medium text-destructive">{message.title}</p><p className="mt-1 text-sm leading-relaxed">{message.action}</p></div>
    </div>
    <details className="mt-3 text-sm">
      <summary className="cursor-pointer text-muted-foreground">Технические подробности</summary>
      <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-background/70 p-3 text-xs text-muted-foreground">{error}</pre>
    </details>
  </div>;
}
