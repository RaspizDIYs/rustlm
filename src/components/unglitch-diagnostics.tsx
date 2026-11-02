"use client";
import { useId, useState } from "react";
import { RefreshCw, Download, ChevronDown } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { WithTooltip } from "@/components/ui/with-tooltip";
import { diagnosticArea, diagnosticOutcome, diagnosticUiLabel, type ClientDiagnostics } from "@/lib/unglitch";

export function UnglitchDiagnostics({ report, busy, onProbe, onExport }: {
  report: ClientDiagnostics | null; busy: boolean; onProbe: () => void; onExport: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const detailId = useId();
  return <Card>
    <CardHeader className="flex flex-row items-center justify-between gap-2">
      <CardTitle><Button size="sm" variant="ghost" aria-expanded={expanded} aria-controls={detailId} onClick={() => setExpanded(!expanded)}>
        Диагностика экранов<ChevronDown className={expanded ? "rotate-180" : undefined} />
      </Button></CardTitle>
      <div className="flex items-center gap-1">
        <WithTooltip label="Снять замер UI Automation"><Button size="icon-sm" variant="outline" aria-label="Снять замер UI Automation" disabled={busy} onClick={onProbe}><RefreshCw /></Button></WithTooltip>
        <WithTooltip label="Сохранить отчёт"><Button size="icon-sm" variant="outline" aria-label="Сохранить диагностический отчёт" disabled={busy || !report} onClick={onExport}><Download /></Button></WithTooltip>
      </div>
    </CardHeader>
    <CardContent className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline">UIA: {diagnosticUiLabel(report?.ui)}</Badge>
        {report?.ui.screen && <Badge variant="secondary">{diagnosticArea(report.ui.screen)}</Badge>}
        {!!report?.ui.progress && <Badge variant="outline">Индикаторов: {report.ui.progress}</Badge>}
        {!!report?.ui.waiting_ms && <Badge variant={report.ui.waiting_ms >= 30000 ? "secondary" : "outline"}>Ожидание: {Math.floor(report.ui.waiting_ms / 1000)} с</Badge>}
      </div>
      <p className="text-xs text-muted-foreground">UIA видит доступные элементы; замеры LCU относятся к запросам LM.</p>
      <div id={detailId} hidden={!expanded}>
      {!!report?.requests.length && <Table>
        <TableHeader><TableRow><TableHead>Раздел</TableHead><TableHead>Ответ</TableHead><TableHead>Ошибки</TableHead></TableRow></TableHeader>
        <TableBody>{report.requests.map(item => <TableRow key={item.area}>
          <TableCell>{diagnosticArea(item.area)}</TableCell><TableCell>{item.last_ms} мс</TableCell><TableCell>{item.errors} / {item.requests}</TableCell>
        </TableRow>)}</TableBody>
      </Table>}
      {!!report?.events.length && <div className="flex max-h-32 flex-col gap-1 overflow-y-auto" aria-label="Последние диагностические события">
        {report.events.slice(0, 6).map(event => <div key={event.id} className="flex flex-wrap items-center gap-2 text-xs">
          <span>{diagnosticArea(event.area)}</span><Badge variant="outline">{diagnosticOutcome(event.outcome)}</Badge>
          <span className="text-muted-foreground">{event.duration_ms} мс{event.status ? ` · HTTP ${event.status}` : ""}</span>
        </div>)}
      </div>}
      </div>
    </CardContent>
  </Card>;
}
