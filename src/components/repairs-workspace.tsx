"use client";
import { useState } from "react";
import { Monitor, Network } from "lucide-react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import UnglitchPanel from "@/components/unglitch-panel";
import RepairToolsPanel from "@/components/repair-tools-panel";

export function RepairsWorkspace({ initialTool = "client" }: { initialTool?: "client" | "network" }) {
  const [tool, setTool] = useState(initialTool);
  return <section className="repairs-workspace flex min-h-0 w-full max-w-4xl flex-col gap-3">
    <Tabs value={tool} onValueChange={value => setTool(value as "client" | "network")}>
      <TabsList variant="line" aria-label="Инструменты" className="h-8 w-fit"><TabsTrigger value="client" data-tour="repair-client"><Monitor/>Клиент</TabsTrigger><TabsTrigger value="network" data-tour="repair-network"><Network/>Подключение</TabsTrigger></TabsList>
      <TabsContent value="client" keepMounted className="mt-2 data-[hidden]:hidden"><UnglitchPanel/></TabsContent>
      <TabsContent value="network" keepMounted className="mt-2 data-[hidden]:hidden"><RepairToolsPanel/></TabsContent>
    </Tabs>
  </section>;
}
