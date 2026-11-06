"use client";
import { useState } from "react";
import { Shield } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { WithTooltip } from "@/components/ui/with-tooltip";
import { rankBadge } from "@/lib/rank";
export function RankBadge({display}:{display:string}) {
 const {tier,label}=rankBadge(display);
 const [failed,setFailed]=useState("");
 return <WithTooltip label={display}><Badge variant="secondary" className="h-7 gap-1.5 px-1.5 py-0.5 text-xs" aria-label={display}>
 {tier && failed!==tier ? <img src={`/ranks/${tier}.webp`} alt="" className="size-6 object-contain" onError={()=>setFailed(tier)}/> : <Shield aria-hidden className="size-3.5"/>}
 {label && <span className="tabular-nums">{label}</span>}
 </Badge></WithTooltip>;
}
