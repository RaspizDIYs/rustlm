"use client";
import { Popover as Primitive } from "@base-ui/react/popover";
import { cn } from "@/lib/utils";

export const Popover = Primitive.Root;
export function PopoverContent({ anchor, className, children, ...props }: Primitive.Popup.Props & Pick<Primitive.Positioner.Props, "anchor">) {
  return <Primitive.Portal><Primitive.Positioner anchor={anchor} side="right" align="start" sideOffset={14} collisionPadding={12} className="z-[80]">
    <Primitive.Popup data-slot="popover-content" className={cn("w-72 max-w-[calc(100vw-24px)] rounded-lg border bg-popover p-3 text-popover-foreground shadow-lg outline-none", className)} {...props}>
      {children}
      <Primitive.Arrow className="absolute size-2.5 rotate-45 bg-popover data-[side=right]:-left-1.5 data-[side=right]:border-b data-[side=right]:border-l data-[side=left]:-right-1.5 data-[side=left]:border-t data-[side=left]:border-r data-[side=top]:-bottom-1.5 data-[side=top]:border-b data-[side=top]:border-r data-[side=bottom]:-top-1.5 data-[side=bottom]:border-t data-[side=bottom]:border-l"/>
    </Primitive.Popup>
  </Primitive.Positioner></Primitive.Portal>;
}
