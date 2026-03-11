"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { RunePage, RunePathModel, AutomationSettings } from "@/lib/tauri";

const DDRAGON = "https://ddragon.leagueoflegends.com";

const SPELL_IMAGE_MAP: Record<string, string> = {
  "4": "SummonerFlash",
  "14": "SummonerDot",
  "12": "SummonerTeleport",
  "11": "SummonerSmite",
  "3": "SummonerExhaust",
  "7": "SummonerHeal",
  "6": "SummonerHaste",
  "1": "SummonerBoost",
  "21": "SummonerBarrier",
  "32": "SummonerSnowball",
  "13": "SummonerMana",
};

type SelectionMode = "pick" | "ban";

export default function AutomationPage() {
  const [champions, setChampions] = useState<Record<string, string>>({});
  const [spells, setSpells] = useState<Record<string, string>>({});
  const [runePages, setRunePages] = useState<RunePage[]>([]);
  const [runePaths, setRunePaths] = useState<RunePathModel[]>([]);
  const [version, setVersion] = useState("");
  const [champSearch, setChampSearch] = useState("");
  const [loading, setLoading] = useState(true);

  // Automation settings
  const [autoAcceptEnabled, setAutoAcceptEnabled] = useState(false);
  const [selectedPick, setSelectedPick] = useState("");
  const [selectedBan, setSelectedBan] = useState("");
  const [selectedSpell1, setSelectedSpell1] = useState("");
  const [selectedSpell2, setSelectedSpell2] = useState("");
  const [selectedRunePage, setSelectedRunePage] = useState("");
  const [selectionMode, setSelectionMode] = useState<SelectionMode>("pick");

  const saveTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadData = useCallback(async () => {
    try {
      const tauri = await import("@/lib/tauri");
      const [ver, champs, sp, pages, paths, enabled, settings] = await Promise.all([
        tauri.getDdragonVersion(),
        tauri.getChampions(),
        tauri.getSummonerSpells(),
        tauri.loadRunePages(),
        tauri.getRunePaths(),
        tauri.isAutoAcceptEnabled(),
        tauri.getAutomationSettings(),
      ]);
      setVersion(ver);
      setChampions(champs);
      setSpells(sp);
      setRunePages(pages);
      setRunePaths(paths);
      setAutoAcceptEnabled(enabled);
      if (settings.PickChampion1) setSelectedPick(settings.PickChampion1);
      if (settings.BanChampion) setSelectedBan(settings.BanChampion);
      if (settings.Spell1Id) setSelectedSpell1(String(settings.Spell1Id));
      if (settings.Spell2Id) setSelectedSpell2(String(settings.Spell2Id));
      if (settings.SelectedRunePageName) setSelectedRunePage(settings.SelectedRunePageName);
    } catch {
      // Not in Tauri
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const saveSettings = useCallback(async (overrides: Partial<AutomationSettings>) => {
    if (saveTimeout.current) clearTimeout(saveTimeout.current);
    saveTimeout.current = setTimeout(async () => {
      try {
        const tauri = await import("@/lib/tauri");
        const current = await tauri.getAutomationSettings();
        await tauri.setAutomationSettings({ ...current, ...overrides });
      } catch (e) {
        console.error("Save settings failed:", e);
      }
    }, 300);
  }, []);

  const handleToggleAutoAccept = async (enabled: boolean) => {
    setAutoAcceptEnabled(enabled);
    try {
      const { setAutoAcceptEnabled: setEnabled } = await import("@/lib/tauri");
      await setEnabled(enabled);
    } catch (e) {
      console.error("Toggle auto-accept failed:", e);
    }
  };

  const handleSelectChampion = (displayName: string) => {
    if (selectionMode === "pick") {
      setSelectedPick(displayName);
      saveSettings({ PickChampion1: displayName });
    } else {
      setSelectedBan(displayName);
      saveSettings({ BanChampion: displayName });
    }
  };

  const handleSelectSpell = (spellKey: string) => {
    const keyNum = Number(spellKey);
    if (!selectedSpell1 || (selectedSpell1 && selectedSpell2)) {
      setSelectedSpell1(spellKey);
      setSelectedSpell2("");
      saveSettings({ Spell1Id: keyNum, Spell2Id: null });
    } else {
      if (spellKey === selectedSpell1) return;
      setSelectedSpell2(spellKey);
      saveSettings({ Spell2Id: keyNum });
    }
  };

  const handleDeleteRunePage = async (pageName: string) => {
    try {
      const { deleteRunePage } = await import("@/lib/tauri");
      await deleteRunePage(pageName);
      setRunePages((prev) => prev.filter((p) => p.Name !== pageName));
    } catch (e) {
      console.error("Delete rune page failed:", e);
    }
  };

  const handleSelectRunePage = (pageName: string) => {
    setSelectedRunePage(pageName);
    saveSettings({ SelectedRunePageName: pageName });
  };

  const filteredChampions = Object.entries(champions)
    .filter(([name]) => name.toLowerCase().includes(champSearch.toLowerCase()))
    .sort(([a], [b]) => a.localeCompare(b));

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
            onCheckedChange={handleToggleAutoAccept}
          />
        </div>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        {/* Champion pick/ban */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base flex items-center justify-between">
              <span>Выбор чемпиона</span>
              <div className="flex gap-1">
                <Button
                  variant={selectionMode === "pick" ? "default" : "outline"}
                  size="sm"
                  onClick={() => setSelectionMode("pick")}
                >
                  Пик
                </Button>
                <Button
                  variant={selectionMode === "ban" ? "destructive" : "outline"}
                  size="sm"
                  onClick={() => setSelectionMode("ban")}
                >
                  Бан
                </Button>
              </div>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <Input
              placeholder="Поиск чемпиона..."
              value={champSearch}
              onChange={(e) => setChampSearch(e.target.value)}
            />
            <ScrollArea className="h-48">
              <div className="grid grid-cols-6 gap-1.5">
                {filteredChampions.map(([displayName, englishName]) => {
                  const isSelected = selectionMode === "pick"
                    ? selectedPick === displayName
                    : selectedBan === displayName;
                  return (
                    <button
                      key={englishName}
                      title={displayName}
                      onClick={() => handleSelectChampion(displayName)}
                      className={`relative rounded-md overflow-hidden border-2 transition-colors ${
                        isSelected
                          ? selectionMode === "pick" ? "border-primary" : "border-destructive"
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
                  );
                })}
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
          <CardContent className="space-y-4">
            <div className="grid grid-cols-4 gap-2">
              {Object.entries(spells).map(([name, key]) => {
                const imgName = SPELL_IMAGE_MAP[key] || "SummonerFlash";
                const isSpell1 = selectedSpell1 === key;
                const isSpell2 = selectedSpell2 === key;
                return (
                  <button
                    key={key}
                    title={name}
                    onClick={() => handleSelectSpell(key)}
                    className={`rounded-md overflow-hidden border-2 transition-colors ${
                      isSpell1
                        ? "border-primary"
                        : isSpell2
                          ? "border-blue-400"
                          : "border-transparent hover:border-muted-foreground/30"
                    }`}
                  >
                    <img
                      src={`${DDRAGON}/cdn/${version}/img/spell/${imgName}.png`}
                      alt={name}
                      className="w-full aspect-square object-cover"
                      loading="lazy"
                    />
                  </button>
                );
              })}
            </div>
            <div className="flex gap-4 text-sm">
              <div>
                <span className="text-muted-foreground">D: </span>
                <Badge variant="secondary">
                  {selectedSpell1 ? Object.entries(spells).find(([, k]) => k === selectedSpell1)?.[0] || "—" : "—"}
                </Badge>
              </div>
              <div>
                <span className="text-muted-foreground">F: </span>
                <Badge variant="secondary">
                  {selectedSpell2 ? Object.entries(spells).find(([, k]) => k === selectedSpell2)?.[0] || "—" : "—"}
                </Badge>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Rune pages */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle className="text-base">Страницы рун</CardTitle>
          <span className="text-sm text-muted-foreground">
            {runePages.length} сохранено
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
                const primaryPath = runePaths.find((p) => p.id === page.PrimaryPathId);
                const secondaryPath = runePaths.find((p) => p.id === page.SecondaryPathId);
                const isSelected = selectedRunePage === page.Name;
                return (
                  <div
                    key={page.Name}
                    className={`flex items-center justify-between rounded-lg border p-3 cursor-pointer transition-colors ${
                      isSelected ? "border-primary bg-primary/5" : "border-border hover:border-muted-foreground/30"
                    }`}
                    onClick={() => handleSelectRunePage(page.Name)}
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
                      {isSelected && <Badge variant="secondary" className="text-xs">Выбрано</Badge>}
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDeleteRunePage(page.Name);
                      }}
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
