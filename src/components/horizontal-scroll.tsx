"use client";
import {useEffect,useRef,type ReactNode} from "react";
export function HorizontalScroll({children,label,activeIndex}:{children:ReactNode;label:string;activeIndex?:number}) {
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
      if (!delta || (delta < 0 && element.scrollLeft <= 0) ||
          (delta > 0 && element.scrollLeft >= element.scrollWidth - element.clientWidth - 1)) return;
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

  useEffect(() => {
    if (activeIndex == null) return;
    scroll.current?.querySelector<HTMLElement>(`[data-scroll-index="${activeIndex}"]`)?.scrollIntoView({block:"nearest",inline:"nearest"});
  }, [activeIndex]);
  return <div ref={scroll} className="accounts-matches-scroll min-w-0" role="region" aria-label={label} tabIndex={0}
    onKeyDown={(event)=>{
      if(event.target!==event.currentTarget)return;
      const element=event.currentTarget;
      if(event.key==="ArrowLeft"||event.key==="ArrowRight"){event.preventDefault();element.scrollLeft+=event.key==="ArrowRight"?73:-73;}
      else if(event.key==="Home"||event.key==="End"){event.preventDefault();element.scrollLeft=event.key==="Home"?0:element.scrollWidth;}
    }}>{children}</div>;
}
