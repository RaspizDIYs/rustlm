"use client";

import { useEffect, useState, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import type { RunePage, RunePathModel } from "@/lib/tauri";

const DDRAGON = "https://ddragon.leagueoflegends.com";

export default function AutomationPage() {
  const [champions, setChampions] = useState<Record<string, string>>({});
  const [spells, setSpells] = useState<Record<string, string>>({});
  const [runePages, setRunePages] = useState<RunePage[]>([]);
  const [runePaths, setRunePaths] = useState<RunePathModel[]>([]);
  const [version, setVersion] = useState("");
  const [champSearch, setChampSearch] = useState("");
  const [selectedPick, setSelectedPick] = useState("");
  const [selectedBan, setSelectedBan] = useState("");
  const [autoAcceptEnabled, setAutoAcceptEnabled] = useState(false);
  const [loading, setLoading] = useState(true);

  const loadData = useCallback(async () => {
    try {
      const tauri = await import("@/lib/tauri");
      const [ver, champs, sp, pages, paths] = await Promise.all([
        tauri.getDdragonVersion(),
        tauri.getChampions(),
        tauri.getSummonerSpells(),
        tauri.loadRunePages(),
        tauri.getRunePaths(),
      ]);
      setVersion(ver);
      setChampions(champs);
      setSpells(sp);
      setRunePages(pages);
      setRunePaths(paths);
    } catch {
      // Not in Tauri
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const filteredChampions = Object.entries(champions)
    .filter(([name]) => name.toLowerCase().includes(champSearch.toLowerCase()))
    .sort(([a], [b]) => a.localeCompare(b));

  const handleDeleteRunePage = async (pageName: string) => {
    try {
      const { deleteRunePage } = await import("@/lib/tauri");
      await deleteRunePage(pageName);
      setRunePages((prev) => prev.filter((p) => p.Name !== pageName));
    } catch (e) {
      console.error("Delete rune page failed:", e);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        Загрузка данных Data Dragon...
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Автоматизация</h1>
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">Авто-принятие</span>
          <Switch
            checked={autoAcceptEnabled}
            onCheckedChange={setAutoAcceptEnabled}
          />
        </div>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        {/* Champion pick/ban */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Выбор чемпиона</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <Input
              placeholder="Поиск чемпиона..."
              value={champSearch}
              onChange={(e) => setChampSearch(e.target.value)}
            />
            <ScrollArea className="h-48">
              <div className="grid grid-cols-6 gap-1.5">
                {filteredChampions.map(([displayName, englishName]) => (
                  <button
                    key={englishName}
                    title={displayName}
                    onClick={() => setSelectedPick(displayName)}
                    className={`relative rounded-md overflow-hidden border-2 transition-colors ${
                      selectedPick === displayName
                        ? "border-primary"
                        : "border-transparent hover:border-muted-foreground/30"
                    }`}
                  >
                    <img
                      src={`${DDRAGON}/cdn/${version}/img/champion/${englishName}.png`}
                      alt={displayName}
                      className="w-full aspect-square object-cover"
                      loading="lazy"
                    />
                  </button>
                ))}
              </div>
            </ScrollArea>

            <div className="flex gap-4 text-sm">
              <div>
                <span className="text-muted-foreground">Пик: </span>
                <Badge variant="secondary">{selectedPick || "—"}</Badge>
              </div>
              <div>
                <span className="text-muted-foreground">Бан: </span>
                <Badge variant="outline">{selectedBan || "—"}</Badge>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Summoner spells */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Заклинания призывателя</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-4 gap-2">
              {Object.entries(spells).map(([name, key]) => (
                <button
                  key={key}
                  title={name}
                  className="rounded-md overflow-hidden border border-border hover:border-primary transition-colors"
                >
                  <img
                    src={`${DDRAGON}/cdn/${version}/img/spell/Summoner${key === "4" ? "Flash" : key === "14" ? "Dot" : key === "12" ? "Teleport" : key === "11" ? "Smite" : key === "3" ? "Exhaust" : key === "7" ? "Heal" : key === "6" ? "Haste" : key === "1" ? "Boost" : key === "21" ? "Barrier" : key === "32" ? "Snowball" : "Flash"}.png`}
                    alt={name}
                    className="w-full aspect-square object-cover"
                    loading="lazy"
                  />
                </button>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Rune pages */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle className="text-base">Страницы рун</CardTitle>
          <span className="text-sm text-muted-foreground">
            {runePaths.length} путей загружено
          </span>
        </CardHeader>
        <CardContent>
          {runePages.length === 0 ? (
            <p className="text-sm text-muted-foreground text-center py-4">
              Нет сохранённых страниц рун
            </p>
          ) : (
            <div className="space-y-2">
              {runePages.map((page) => {
                const primaryPath = runePaths.find(
                  (p) => p.id === page.PrimaryPathId
                );
                const secondaryPath = runePaths.find(
                  (p) => p.id === page.SecondaryPathId
                );
                return (
                  <div
                    key={page.Name}
                    className="flex items-center justify-between rounded-lg border border-border p-3"
                  >
                    <div className="flex items-center gap-3">
                      <div className="flex gap-1">
                        {primaryPath && (
                          <img
                            src={`${DDRAGON}/cdn/img/${primaryPath.icon}`}
                            alt={primaryPath.name}
                            className="w-6 h-6"
                          />
                        )}
                        {secondaryPath && (
                          <img
                            src={`${DDRAGON}/cdn/img/${secondaryPath.icon}`}
                            alt={secondaryPath.name}
                            className="w-6 h-6 opacity-70"
                          />
                        )}
                      </div>
                      <span className="text-sm font-medium">{page.Name}</span>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleDeleteRunePage(page.Name)}
                    >
                      Удалить
                    </Button>
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
