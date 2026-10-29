"use client";
import { useState } from "react";
import { Shield } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { WithTooltip } from "@/components/ui/with-tooltip";
import { rankBadge } from "@/lib/rank";
export function RankBadge({display}:{display:string}) {
 const {tier,label}=rankBadge(display);
 const [failed,setFailed]=useState("");
 return <WithTooltip label={display}><Badge variant="secondary" className="gap-1 px-1 text-[11px]" aria-label={display}>
 {tier && failed!==tier ? <img src={`/ranks/${tier}.webp`} alt="" className="size-5 object-contain" onError={()=>setFailed(tier)}/> : <Shield aria-hidden className="size-3.5"/>}
 {label && <span className="tabular-nums">{label}</span>}
 </Badge></WithTooltip>;
}
