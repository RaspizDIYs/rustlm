"use client";

import { TooltipProvider } from "@/components/ui/tooltip";

export function Providers({ children }: { children: React.ReactNode }) {
  return <TooltipProvider delay={250}>{children}</TooltipProvider>;
}
