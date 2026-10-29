"use client";

import { useState, useCallback, useEffect } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { ArrowLeft, Bug, Copy, Check } from "lucide-react";
import { toast } from "sonner";
import { DiscordIcon, GitHubIcon } from "@/components/brand-icons";
import { ChangelogView } from "@/components/changelog-view";

const DEVELOPERS = [
  { name: "@mejaikin", discord: "mejaikin" },
  { name: "@shp1n4t", discord: "shp1n4t" },
];

export default function InfoPage() {
  const [showChangelog, setShowChangelog] = useState(false);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const [appVersion, setAppVersion] = useState("0.1.0");

  useEffect(() => {
    (async () => {
      try {
        const { getVersion } = await import("@tauri-apps/api/app");
        setAppVersion(await getVersion());
      } catch {}
    })();
  }, []);

  const copyDiscord = useCallback(async (discord: string, index: number) => {
    try {
    await navigator.clipboard.writeText(discord);
    setCopiedIndex(index);
    setTimeout(() => setCopiedIndex(null), 2000);
    } catch { toast.error("Не удалось скопировать. Повторите попытку."); }
  }, []);

  const openUrl = useCallback(async (url: string) => {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(url);
    } catch {
      window.open(url, "_blank");
    }
  }, []);

  if (showChangelog) {
    return (
      <div className="space-y-3">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="sm" onClick={() => setShowChangelog(false)}>
            <ArrowLeft className="h-4 w-4 mr-1" /> Назад
          </Button>
          <h1 className="text-lg font-semibold">История изменений</h1>
        </div>
        <Card>
          <CardContent className="pt-6">
            <ChangelogView activeVersion={appVersion} />
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col gap-1"><h1 className="text-lg font-semibold">RustLM</h1><p className="text-xs text-muted-foreground">Менеджер аккаунтов League of Legends</p></div>
        <Button variant="outline" size="sm" onClick={() => setShowChangelog(true)}>Версия {appVersion}</Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Контакты разработчиков</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {DEVELOPERS.map((dev, i) => (
            <div key={i} className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <DiscordIcon className="h-4 w-4 text-muted-foreground" />
                <span className="text-xs font-medium">{dev.name}</span>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => copyDiscord(dev.discord, i)}
              >
                {copiedIndex === i ? (
                  <><Check className="h-3 w-3 mr-1" /> Скопировано</>
                ) : (
                  <><Copy className="h-3 w-3 mr-1" /> Discord</>
                )}
              </Button>
            </div>
          ))}
        </CardContent>
      </Card>

      <div className="flex gap-2 justify-center">
        <Button
          variant="outline"
          size="sm"
          onClick={() => openUrl("https://github.com/RaspizDIYs/rustlm/issues/new")}
        >
          <Bug className="h-4 w-4 mr-1" /> Сообщить о проблеме
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => openUrl("https://github.com/RaspizDIYs/rustlm")}
        >
          <GitHubIcon className="h-4 w-4 mr-1" /> GitHub
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => openUrl("https://discord.gg/9wS4DBMDNB")}
        >
          <DiscordIcon className="h-4 w-4 mr-1" /> Discord
        </Button>
      </div>
    </div>
  );
}
