"use client";

import { useEffect, useState, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import type { RunePage, RunePathModel, RuneModel } from "@/lib/tauri";

const DDRAGON = "https://ddragon.leagueoflegends.com";

interface RunePageEditorProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  runePaths: RunePathModel[];
  editPage?: RunePage | null;
  onSave: (page: RunePage) => void;
}

export function RunePageEditor({
  open,
  onOpenChange,
  runePaths,
  editPage,
  onSave,
}: RunePageEditorProps) {
  const [name, setName] = useState("");
  const [primaryPathId, setPrimaryPathId] = useState(0);
  const [secondaryPathId, setSecondaryPathId] = useState(0);
  const [keystoneId, setKeystoneId] = useState(0);
  const [primarySlots, setPrimarySlots] = useState([0, 0, 0]);
  const [secondarySlots, setSecondarySlots] = useState<number[]>([]);
  const [statMods, setStatMods] = useState([0, 0, 0]);

  const [statModRows, setStatModRows] = useState<[RuneModel[], RuneModel[], RuneModel[]]>([[], [], []]);

  const loadStatMods = useCallback(async () => {
    try {
      const tauri = await import("@/lib/tauri");
      const [r1, r2, r3] = await Promise.all([
        tauri.getStatModsRow1(),
        tauri.getStatModsRow2(),
        tauri.getStatModsRow3(),
      ]);
      setStatModRows([r1, r2, r3]);
    } catch {}
  }, []);

  useEffect(() => {
    loadStatMods();
  }, [loadStatMods]);

  useEffect(() => {
    if (editPage) {
      setName(editPage.Name);
      setPrimaryPathId(editPage.PrimaryPathId);
      setSecondaryPathId(editPage.SecondaryPathId);
      setKeystoneId(editPage.PrimaryKeystoneId);
      setPrimarySlots([editPage.PrimarySlot1Id, editPage.PrimarySlot2Id, editPage.PrimarySlot3Id]);
      setSecondarySlots([editPage.SecondarySlot1Id, editPage.SecondarySlot2Id].filter((id) => id > 0));
      setStatMods([editPage.StatMod1Id, editPage.StatMod2Id, editPage.StatMod3Id]);
    } else {
      setName("");
      setPrimaryPathId(0);
      setSecondaryPathId(0);
      setKeystoneId(0);
      setPrimarySlots([0, 0, 0]);
      setSecondarySlots([]);
      setStatMods([0, 0, 0]);
    }
  }, [editPage, open]);

  const primaryPath = runePaths.find((p) => p.id === primaryPathId);
  const secondaryPath = runePaths.find((p) => p.id === secondaryPathId);

  const keystones = primaryPath?.slots[0]?.runes || [];
  const primaryRuneSlots = primaryPath?.slots.slice(1) || [];
  const secondaryRuneSlots = secondaryPath?.slots.slice(1) || [];

  const handleSelectPrimaryPath = (pathId: number) => {
    setPrimaryPathId(pathId);
    if (pathId === secondaryPathId) setSecondaryPathId(0);
    setKeystoneId(0);
    setPrimarySlots([0, 0, 0]);
  };

  const handleSelectSecondaryPath = (pathId: number) => {
    setSecondaryPathId(pathId);
    setSecondarySlots([]);
  };

  const handleSecondaryRuneClick = (rune: RuneModel, slotRunes: RuneModel[]) => {
    const slotRuneIds = slotRunes.map((r) => r.id);
    const isSelected = secondarySlots.includes(rune.id);

    if (isSelected) {
      // Deselect
      setSecondarySlots(secondarySlots.filter((id) => id !== rune.id));
    } else {
      // Remove any existing selection from this same row, then add new
      const withoutThisRow = secondarySlots.filter((id) => !slotRuneIds.includes(id));
      const next = [...withoutThisRow, rune.id];
      // Max 2 secondary runes
      setSecondarySlots(next.slice(-2));
    }
  };

  const handleSave = () => {
    if (!name.trim()) return;
    onSave({
      Name: name.trim(),
      PrimaryPathId: primaryPathId,
      SecondaryPathId: secondaryPathId,
      PrimaryKeystoneId: keystoneId,
      PrimarySlot1Id: primarySlots[0],
      PrimarySlot2Id: primarySlots[1],
      PrimarySlot3Id: primarySlots[2],
      SecondarySlot1Id: secondarySlots[0] || 0,
      SecondarySlot2Id: secondarySlots[1] || 0,
      SecondarySlot3Id: 0,
      StatMod1Id: statMods[0],
      StatMod2Id: statMods[1],
      StatMod3Id: statMods[2],
    });
    onOpenChange(false);
  };

  const runeIcon = (rune: RuneModel, selected: boolean, onClick: () => void, size = "w-8 h-8") => (
    <button
      key={rune.id}
      onClick={onClick}
      title={rune.name}
      className={`rounded-full border-2 transition-colors ${
        selected ? "border-primary" : "border-transparent opacity-60 hover:opacity-100"
      }`}
    >
      <img
        src={`${DDRAGON}/cdn/img/${rune.icon}`}
        alt={rune.name}
        className={`${size} rounded-full`}
      />
    </button>
  );

  const pathIcon = (path: RunePathModel, selected: boolean, onClick: () => void) => (
    <button
      key={path.id}
      onClick={onClick}
      className={`rounded-full border-2 p-0.5 transition-all ${
        selected ? "border-primary" : "border-transparent opacity-50 hover:opacity-80"
      }`}
    >
      <img src={`${DDRAGON}/cdn/img/${path.icon}`} alt={path.name} className="w-8 h-8" title={path.name} />
    </button>
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl select-none">
        <DialogHeader>
          <DialogTitle>{editPage ? "Редактировать страницу рун" : "Новая страница рун"}</DialogTitle>
        </DialogHeader>

        {runePaths.length === 0 ? (
          <div className="space-y-6 py-2">
            <Skeleton className="h-9 w-full rounded-md" />
            <div className="grid grid-cols-2 gap-6">
              {/* Left column skeleton */}
              <div className="space-y-4">
                <div>
                  <Skeleton className="h-3 w-24 mb-2" />
                  <div className="flex gap-2 justify-center">
                    {Array.from({ length: 5 }).map((_, i) => (
                      <Skeleton key={i} className="w-8 h-8 rounded-full" />
                    ))}
                  </div>
                </div>
                <div>
                  <Skeleton className="h-3 w-28 mb-2" />
                  <div className="flex gap-2 justify-center">
                    {Array.from({ length: 4 }).map((_, i) => (
                      <Skeleton key={i} className="w-10 h-10 rounded-full" />
                    ))}
                  </div>
                </div>
                {Array.from({ length: 3 }).map((_, i) => (
                  <div key={i} className="flex gap-2 justify-center">
                    {Array.from({ length: 3 }).map((_, j) => (
                      <Skeleton key={j} className="w-8 h-8 rounded-full" />
                    ))}
                  </div>
                ))}
              </div>
              {/* Right column skeleton */}
              <div className="space-y-4">
                <div>
                  <Skeleton className="h-3 w-32 mb-2" />
                  <div className="flex gap-2 justify-center">
                    {Array.from({ length: 4 }).map((_, i) => (
                      <Skeleton key={i} className="w-8 h-8 rounded-full" />
                    ))}
                  </div>
                </div>
                {Array.from({ length: 3 }).map((_, i) => (
                  <div key={i} className="flex gap-2 justify-center">
                    {Array.from({ length: 3 }).map((_, j) => (
                      <Skeleton key={j} className="w-8 h-8 rounded-full" />
                    ))}
                  </div>
                ))}
                <div className="pt-2 border-t border-border">
                  <Skeleton className="h-3 w-12 mb-2" />
                  {Array.from({ length: 3 }).map((_, i) => (
                    <div key={i} className="flex gap-2 justify-center mb-1.5">
                      {Array.from({ length: 3 }).map((_, j) => (
                        <Skeleton key={j} className="w-6 h-6 rounded-full" />
                      ))}
                    </div>
                  ))}
                </div>
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Skeleton className="h-9 w-20 rounded-md" />
              <Skeleton className="h-9 w-24 rounded-md" />
            </div>
          </div>
        ) : (
          <>
            <Input
              className="select-text"
              placeholder="Название страницы"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />

            <ScrollArea className="max-h-[55vh]">
              <div className="grid grid-cols-2 gap-6 pr-2">
                {/* LEFT: Primary Path */}
                <div className="space-y-4">
                  <div>
                    <p className="text-xs text-muted-foreground mb-2">Основной путь</p>
                    <div className="flex gap-2 justify-center">
                      {runePaths.map((path) => pathIcon(path, primaryPathId === path.id, () => handleSelectPrimaryPath(path.id)))}
                    </div>
                  </div>

                  {primaryPath && (
                    <>
                      <div>
                        <p className="text-xs text-muted-foreground mb-2">Ключевой камень</p>
                        <div className="flex gap-2 justify-center">
                          {keystones.map((rune) => runeIcon(rune, keystoneId === rune.id, () => setKeystoneId(rune.id), "w-10 h-10"))}
                        </div>
                      </div>

                      {primaryRuneSlots.map((slot, i) => (
                        <div key={`primary-${i}`}>
                          <div className="flex gap-2 justify-center">
                            {slot.runes.map((rune) =>
                              runeIcon(rune, primarySlots[i] === rune.id, () => {
                                const next = [...primarySlots];
                                next[i] = rune.id;
                                setPrimarySlots(next);
                              })
                            )}
                          </div>
                        </div>
                      ))}
                    </>
                  )}

                  {!primaryPath && (
                    <>
                      <div>
                        <p className="text-xs text-muted-foreground mb-2">Ключевой камень</p>
                        <div className="flex gap-2 justify-center">
                          {Array.from({ length: 4 }).map((_, i) => (
                            <div key={i} className="w-10 h-10 rounded-full border-2 border-dashed border-muted-foreground/20" />
                          ))}
                        </div>
                      </div>
                      {Array.from({ length: 3 }).map((_, i) => (
                        <div key={i} className="flex gap-2 justify-center">
                          {Array.from({ length: 3 }).map((_, j) => (
                            <div key={j} className="w-8 h-8 rounded-full border-2 border-dashed border-muted-foreground/20" />
                          ))}
                        </div>
                      ))}
                    </>
                  )}
                </div>

                {/* RIGHT: Secondary Path + Stat Mods */}
                <div className="space-y-4">
                  {primaryPath ? (
                    <>
                      <div>
                        <p className="text-xs text-muted-foreground mb-2">Дополнительный путь</p>
                        <div className="flex gap-2 justify-center">
                          {runePaths
                            .filter((p) => p.id !== primaryPathId)
                            .map((path) => pathIcon(path, secondaryPathId === path.id, () => handleSelectSecondaryPath(path.id)))}
                        </div>
                      </div>

                      {secondaryPath ? secondaryRuneSlots.map((slot, i) => (
                        <div key={`secondary-${i}`}>
                          <div className="flex gap-2 justify-center">
                            {slot.runes.map((rune) =>
                              runeIcon(
                                rune,
                                secondarySlots.includes(rune.id),
                                () => handleSecondaryRuneClick(rune, slot.runes)
                              )
                            )}
                          </div>
                        </div>
                      )) : (
                        Array.from({ length: 3 }).map((_, i) => (
                          <div key={i} className="flex gap-2 justify-center">
                            {Array.from({ length: 3 }).map((_, j) => (
                              <div key={j} className="w-8 h-8 rounded-full border-2 border-dashed border-muted-foreground/20" />
                            ))}
                          </div>
                        ))
                      )}

                      {/* Stat Mods */}
                      {statModRows[0].length > 0 && (
                        <div className="pt-2 border-t border-border">
                          <p className="text-xs text-muted-foreground mb-2">Статы</p>
                          {statModRows.map((row, rowIdx) => (
                            <div key={`stat-${rowIdx}`} className="flex gap-2 justify-center mb-1.5">
                              {row.map((mod) => (
                                <button
                                  key={mod.id}
                                  onClick={() => {
                                    const next = [...statMods];
                                    next[rowIdx] = mod.id;
                                    setStatMods(next);
                                  }}
                                  title={mod.name}
                                  className={`rounded-full border-2 transition-all ${
                                    statMods[rowIdx] === mod.id
                                      ? "border-primary"
                                      : "border-transparent opacity-50 hover:opacity-80"
                                  }`}
                                >
                                  <img src={`${DDRAGON}/cdn/img/${mod.icon}`} alt={mod.name} className="w-6 h-6 rounded-full" />
                                </button>
                              ))}
                            </div>
                          ))}
                        </div>
                      )}
                    </>
                  ) : (
                    <>
                      <div>
                        <p className="text-xs text-muted-foreground mb-2">Дополнительный путь</p>
                        <div className="flex gap-2 justify-center">
                          {Array.from({ length: 4 }).map((_, i) => (
                            <div key={i} className="w-8 h-8 rounded-full border-2 border-dashed border-muted-foreground/20 p-0.5" />
                          ))}
                        </div>
                      </div>
                      {Array.from({ length: 3 }).map((_, i) => (
                        <div key={i} className="flex gap-2 justify-center">
                          {Array.from({ length: 3 }).map((_, j) => (
                            <div key={j} className="w-8 h-8 rounded-full border-2 border-dashed border-muted-foreground/20" />
                          ))}
                        </div>
                      ))}
                      <div className="pt-2 border-t border-border">
                        <p className="text-xs text-muted-foreground mb-2">Статы</p>
                        {Array.from({ length: 3 }).map((_, i) => (
                          <div key={i} className="flex gap-2 justify-center mb-1.5">
                            {Array.from({ length: 3 }).map((_, j) => (
                              <div key={j} className="w-6 h-6 rounded-full border-2 border-dashed border-muted-foreground/20" />
                            ))}
                          </div>
                        ))}
                      </div>
                    </>
                  )}
                </div>
              </div>
            </ScrollArea>

            <DialogFooter>
              <Button variant="outline" onClick={() => onOpenChange(false)}>Отмена</Button>
              <Button onClick={handleSave} disabled={!name.trim() || !primaryPathId}>
                Сохранить
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
