"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { RunePageEditor } from "@/components/rune-page-editor";
import { Skeleton } from "@/components/ui/skeleton";
import { X } from "lucide-react";
import { AutoAcceptSwitch } from "@/components/auto-accept-provider";
import { WithTooltip } from "@/components/ui/with-tooltip";
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

export default function AutomationPage() {
  const [champions, setChampions] = useState<Record<string, string>>({});
  const [spells, setSpells] = useState<Record<string, string>>({});
  const [runePages, setRunePages] = useState<RunePage[]>([]);
  const [runePaths, setRunePaths] = useState<RunePathModel[]>([]);
  const [version, setVersion] = useState("");
  const [champSearch, setChampSearch] = useState("");
  const [loading, setLoading] = useState(true);

  // Automation settings
  const [autoPickEnabled, setAutoPickEnabled] = useState(true);
  const [autoBanEnabled, setAutoBanEnabled] = useState(true);
  const [autoSpellsEnabled, setAutoSpellsEnabled] = useState(true);
  const [autoRunesEnabled, setAutoRunesEnabled] = useState(true);
  const [selectedPick, setSelectedPick] = useState("");
  const [selectedBan, setSelectedBan] = useState("");
  const [selectedSpell1, setSelectedSpell1] = useState("");
  const [selectedSpell2, setSelectedSpell2] = useState("");
  const [selectedRunePage, setSelectedRunePage] = useState("");
  const [runeEditorOpen, setRuneEditorOpen] = useState(false);
  const [editingRunePage, setEditingRunePage] = useState<RunePage | null>(null);

  const saveTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadData = useCallback(async () => {
    try {
      const tauri = await import("@/lib/tauri");
      const [ver, champs, sp, pages, paths, settings] = await Promise.all([
        tauri.getDdragonVersion(),
        tauri.getChampions(),
        tauri.getSummonerSpells(),
        tauri.loadRunePages(),
        tauri.getRunePaths(),
        tauri.getAutomationSettings(),
      ]);
      setVersion(ver);
      setChampions(champs);
      setSpells(sp);
      setRunePages(pages);
      setRunePaths(paths);
      setSelectedPick(settings.PickChampion1 ?? "");
      setSelectedBan(settings.BanChampion ?? "");
      setSelectedSpell1(
        settings.Spell1Id != null ? String(settings.Spell1Id) : ""
      );
      setSelectedSpell2(
        settings.Spell2Id != null ? String(settings.Spell2Id) : ""
      );
      if (settings.SelectedRunePageName) setSelectedRunePage(settings.SelectedRunePageName);
      if (settings.AutoPickEnabled !== undefined) setAutoPickEnabled(settings.AutoPickEnabled);
      if (settings.AutoBanEnabled !== undefined) setAutoBanEnabled(settings.AutoBanEnabled);
      if (settings.AutoSpellsEnabled !== undefined) setAutoSpellsEnabled(settings.AutoSpellsEnabled);
      if (settings.AutoRunesEnabled !== undefined) setAutoRunesEnabled(settings.AutoRunesEnabled);
    } catch {
      // Not in Tauri
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    let un: (() => void) | undefined;
    void (async () => {
      const { isTauri } = await import("@tauri-apps/api/core");
      if (!isTauri()) return;
      const { listen } = await import("@tauri-apps/api/event");
      un = await listen("cloud-sync-complete", () => {
        void loadData();
      });
    })();
    return () => {
      un?.();
    };
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

  const handleSelectChampion = (displayName: string) => {
    if (!autoPickEnabled && !autoBanEnabled) return;

    if (autoPickEnabled && !autoBanEnabled) {
      setSelectedPick(displayName);
      setSelectedBan("");
      saveSettings({
        PickChampion1: displayName,
        BanChampion: null,
        BanChampionId: null,
      });
      return;
    }

    if (!autoPickEnabled && autoBanEnabled) {
      setSelectedBan(displayName);
      saveSettings({ BanChampion: displayName });
      return;
    }

    if (!selectedPick || (selectedPick && selectedBan)) {
      setSelectedPick(displayName);
      setSelectedBan("");
      saveSettings({
        PickChampion1: displayName,
        BanChampion: null,
        BanChampionId: null,
      });
    } else {
      if (displayName === selectedPick) return;
      setSelectedBan(displayName);
      saveSettings({ BanChampion: displayName });
    }
  };

  const clearPick = () => {
    setSelectedPick("");
    saveSettings({
      PickChampion1: null,
      PickChampion1Id: null,
    });
  };

  const clearBan = () => {
    setSelectedBan("");
    saveSettings({
      BanChampion: null,
      BanChampionId: null,
    });
  };

  const clearSpell1 = () => {
    setSelectedSpell1("");
    saveSettings({ Spell1Id: null });
  };

  const clearSpell2 = () => {
    setSelectedSpell2("");
    saveSettings({ Spell2Id: null });
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

  const handleSaveRunePage = async (page: RunePage) => {
    try {
      const { saveRunePage } = await import("@/lib/tauri");
      await saveRunePage(page);
      const { loadRunePages } = await import("@/lib/tauri");
      setRunePages(await loadRunePages());
    } catch (e) {
      console.error("Save rune page failed:", e);
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
      <div className="space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <Skeleton className="h-8 w-48" />
          <AutoAcceptSwitch />
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-1">
          {/* Champion pick/ban skeleton */}
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <Skeleton className="h-5 w-32" />
                  <Skeleton className="h-5 w-9 rounded-full" />
                  <Skeleton className="h-5 w-9 rounded-full" />
                </div>
                <div className="w-8" />
              </div>
            </CardHeader>
            <CardContent className="space-y-4">
              <Skeleton className="h-9 w-full rounded-md" />
              <div className="h-52 md:h-56 lg:h-60 xl:h-64 min-h-52 overflow-hidden">
                <div className="grid justify-start gap-2 grid-cols-[repeat(auto-fill,3.5rem)]">
                  {Array.from({ length: 18 }).map((_, i) => (
                    <Skeleton key={i} className="h-14 w-14 shrink-0 rounded-md" />
                  ))}
                </div>
              </div>
              <div className="flex gap-4">
                <Skeleton className="h-5 w-20" />
                <Skeleton className="h-5 w-20" />
              </div>
            </CardContent>
          </Card>

          {/* Summoner spells skeleton */}
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <Skeleton className="h-5 w-44" />
                <Skeleton className="h-5 w-9 rounded-full" />
              </div>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex flex-wrap gap-1.5">
                {Array.from({ length: 11 }).map((_, i) => (
                  <Skeleton key={i} className="w-10 h-10 rounded-md" />
                ))}
              </div>
              <div className="flex gap-4">
                <Skeleton className="h-5 w-16" />
                <Skeleton className="h-5 w-16" />
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Rune pages skeleton */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between">
            <div className="flex items-center gap-3">
              <Skeleton className="h-5 w-28" />
              <Skeleton className="h-5 w-9 rounded-full" />
            </div>
            <div className="flex items-center gap-2">
              <Skeleton className="h-4 w-20" />
              <Skeleton className="h-8 w-16 rounded-md" />
            </div>
          </CardHeader>
          <CardContent className="space-y-2">
            {Array.from({ length: 3 }).map((_, i) => (
              <Skeleton key={i} className="h-14 w-full rounded-lg" />
            ))}
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Автоматизация</h1>
        <AutoAcceptSwitch />
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-1">
        {/* Champion pick/ban */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base flex items-center justify-between gap-2">
              <div className="flex flex-wrap items-center gap-3">
                <span>Выбор чемпиона</span>
                <div className="flex items-center gap-1.5">
                  <span className="text-xs text-muted-foreground font-normal">Пик</span>
                  <Switch
                    checked={autoPickEnabled}
                    onCheckedChange={(v) => { setAutoPickEnabled(v); saveSettings({ AutoPickEnabled: v }); }}
                    className="scale-75"
                  />
                  <span className="text-xs text-muted-foreground font-normal">Бан</span>
                  <Switch
                    checked={autoBanEnabled}
                    onCheckedChange={(v) => { setAutoBanEnabled(v); saveSettings({ AutoBanEnabled: v }); }}
                    className="scale-75"
                  />
                </div>
              </div>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <Input
              placeholder="Поиск чемпиона..."
              value={champSearch}
              onChange={(e) => setChampSearch(e.target.value)}
            />
            <ScrollArea className="h-52 md:h-56 lg:h-60 xl:h-64 min-h-52">
              <div className="grid justify-start gap-2 grid-cols-[repeat(auto-fill,3.5rem)] auto-rows-[3.5rem]">
                {filteredChampions.map(([displayName, englishName]) => {
                  const isPick = autoPickEnabled && selectedPick === displayName;
                  const isBan = autoBanEnabled && selectedBan === displayName;
                  return (
                    <WithTooltip key={englishName} label={displayName}>
                      <button
                        type="button"
                        onClick={() => handleSelectChampion(displayName)}
                        className={`relative h-14 w-14 shrink-0 overflow-hidden rounded-md border-2 p-0 transition-colors ${
                          isPick
                            ? "border-primary"
                            : isBan
                              ? "border-destructive"
                              : "border-transparent hover:border-muted-foreground/30"
                        }`}
                      >
                        <img
                          src={`${DDRAGON}/cdn/${version}/img/champion/${englishName}.png`}
                          alt={displayName}
                          className="h-full w-full object-cover"
                          loading="lazy"
                        />
                      </button>
                    </WithTooltip>
                  );
                })}
              </div>
            </ScrollArea>

            {(autoPickEnabled || autoBanEnabled) ? (
              <div className="flex flex-wrap gap-x-4 gap-y-2 text-sm">
                {autoPickEnabled ? (
                  <div className="inline-flex items-stretch overflow-hidden rounded-md border border-border bg-muted/40">
                    <span className="flex items-center px-2.5 py-1 text-xs font-bold text-muted-foreground">
                      Пик
                    </span>
                    {selectedPick ? (
                      <>
                        <div className="w-px shrink-0 self-stretch bg-border" aria-hidden />
                        <div className="flex items-center gap-1.5 px-2.5 py-1">
                          <span className="text-foreground">{selectedPick}</span>
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon"
                            className="h-4 w-4 shrink-0 p-0 text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
                            aria-label="Сбросить пик"
                            onClick={clearPick}
                          >
                            <X className="h-2.5 w-2.5" strokeWidth={2.5} />
                          </Button>
                        </div>
                      </>
                    ) : null}
                  </div>
                ) : null}
                {autoBanEnabled ? (
                  <div className="inline-flex items-stretch overflow-hidden rounded-md border border-border bg-muted/40">
                    <span className="flex items-center px-2.5 py-1 text-xs font-bold text-muted-foreground">
                      Бан
                    </span>
                    {selectedBan ? (
                      <>
                        <div className="w-px shrink-0 self-stretch bg-border" aria-hidden />
                        <div className="flex items-center gap-1.5 px-2.5 py-1">
                          <span className="text-foreground">{selectedBan}</span>
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon"
                            className="h-4 w-4 shrink-0 p-0 text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
                            aria-label="Сбросить бан"
                            onClick={clearBan}
                          >
                            <X className="h-2.5 w-2.5" strokeWidth={2.5} />
                          </Button>
                        </div>
                      </>
                    ) : null}
                  </div>
                ) : null}
              </div>
            ) : null}
          </CardContent>
        </Card>

        {/* Summoner spells */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base flex items-center justify-between">
              <span>Заклинания призывателя</span>
              <Switch
                checked={autoSpellsEnabled}
                onCheckedChange={(v) => { setAutoSpellsEnabled(v); saveSettings({ AutoSpellsEnabled: v }); }}
              />
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex flex-wrap gap-1.5">
              {Object.entries(spells).map(([name, key]) => {
                const imgName = SPELL_IMAGE_MAP[key] || "SummonerFlash";
                const isSpell1 = selectedSpell1 === key;
                const isSpell2 = selectedSpell2 === key;
                return (
                  <WithTooltip key={key} label={name}>
                    <button
                      type="button"
                      onClick={() => handleSelectSpell(key)}
                      className={`rounded-md overflow-hidden border-2 transition-colors w-10 h-10 ${
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
                        className="w-full h-full object-cover"
                        loading="lazy"
                      />
                    </button>
                  </WithTooltip>
                );
              })}
            </div>
            <div className="flex flex-wrap items-center gap-2 text-sm">
              <div className="inline-flex items-stretch overflow-hidden rounded-md border border-border bg-muted/40">
                <span className="flex items-center px-2.5 py-1 font-mono text-xs font-bold tabular-nums text-muted-foreground">
                  D
                </span>
                {selectedSpell1 ? (
                  <>
                    <div className="w-px shrink-0 self-stretch bg-border" aria-hidden />
                    <div className="flex items-center gap-1.5 px-2.5 py-1">
                      <span className="text-foreground">
                        {Object.entries(spells).find(([, k]) => k === selectedSpell1)?.[0] || "—"}
                      </span>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-4 w-4 shrink-0 p-0 text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
                        aria-label="Сбросить заклинание D"
                        onClick={clearSpell1}
                      >
                        <X className="h-2.5 w-2.5" strokeWidth={2.5} />
                      </Button>
                    </div>
                  </>
                ) : null}
              </div>
              <div className="inline-flex items-stretch overflow-hidden rounded-md border border-border bg-muted/40">
                <span className="flex items-center px-2.5 py-1 font-mono text-xs font-bold tabular-nums text-muted-foreground">
                  F
                </span>
                {selectedSpell2 ? (
                  <>
                    <div className="w-px shrink-0 self-stretch bg-border" aria-hidden />
                    <div className="flex items-center gap-1.5 px-2.5 py-1">
                      <span className="text-foreground">
                        {Object.entries(spells).find(([, k]) => k === selectedSpell2)?.[0] || "—"}
                      </span>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-4 w-4 shrink-0 p-0 text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
                        aria-label="Сбросить заклинание F"
                        onClick={clearSpell2}
                      >
                        <X className="h-2.5 w-2.5" strokeWidth={2.5} />
                      </Button>
                    </div>
                  </>
                ) : null}
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Rune pages */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div className="flex items-center gap-3">
            <CardTitle className="text-base">Страницы рун</CardTitle>
            <Switch
              checked={autoRunesEnabled}
              onCheckedChange={(v) => { setAutoRunesEnabled(v); saveSettings({ AutoRunesEnabled: v }); }}
            />
          </div>
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">
              {runePages.length} сохранено
            </span>
            <Button
              size="sm"
              onClick={() => { setEditingRunePage(null); setRuneEditorOpen(true); }}
            >
              Создать
            </Button>
          </div>
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
                    <div className="flex gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingRunePage(page);
                          setRuneEditorOpen(true);
                        }}
                      >
                        Изменить
                      </Button>
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
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <RunePageEditor
        open={runeEditorOpen}
        onOpenChange={setRuneEditorOpen}
        runePaths={runePaths}
        editPage={editingRunePage}
        onSave={handleSaveRunePage}
      />
    </div>
  );
}
