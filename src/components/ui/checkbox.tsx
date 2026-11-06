"use client";
import { Checkbox as Primitive } from "@base-ui/react/checkbox";
import { Check } from "lucide-react";
import { cn } from "@/lib/utils";
export function Checkbox({ className, ...props }: Primitive.Root.Props) {
  return <Primitive.Root data-slot="checkbox" className={cn("inline-flex size-4 shrink-0 items-center justify-center rounded border border-input bg-background outline-none focus-visible:ring-2 focus-visible:ring-ring data-checked:border-primary data-checked:bg-primary data-checked:text-primary-foreground disabled:opacity-50", className)} {...props}>
    <Primitive.Indicator><Check className="size-3"/></Primitive.Indicator>
  </Primitive.Root>;
}
