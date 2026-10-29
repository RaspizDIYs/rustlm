"use client";

import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { X, Search, ArrowLeftRight, LockKeyhole } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectGroup,
  SelectItem,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { RunePageEditor } from "@/components/rune-page-editor";
import { AutoAcceptSwitch } from "@/components/auto-accept-provider";
import { WithTooltip } from "@/components/ui/with-tooltip";
import {
  MODES,
  ROLES,
  editorProfile,
  allowedSpells,
  pickCount,
  jungleSpells,
  type AutomationMode,
} from "@/lib/automation";
import { cn } from "@/lib/utils";
import { ChampionGrid } from "@/components/champion-grid";
import { HighlightText } from "@/components/highlight-text";
import { LolRoleIcon } from "@/components/lol-role-icon";
import type {
  RunePage,
  RunePathModel,
  AutomationSettings,
  AutomationProfile,
} from "@/lib/tauri";

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
  const [settings, setSettings] = useState<AutomationSettings | null>(null);
  const latest = useRef<AutomationSettings | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queue = useRef(Promise.resolve());
  const revision = useRef(0);
  const dirty = useRef(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [champions, setChampions] = useState<Record<string, string>>({});
  const [spells, setSpells] = useState<Record<string, string>>({});
  const [version, setVersion] = useState("");
  const [runePages, setRunePages] = useState<RunePage[]>([]);
  const [runePaths, setRunePaths] = useState<RunePathModel[]>([]);
  const [runeEditorOpen, setRuneEditorOpen] = useState(false);
  const [editingRunePage, setEditingRunePage] = useState<RunePage | null>(null);
  const [mode, setMode] = useState<AutomationMode>("classic");
  const [role, setRole] = useState("top");
  const [slot, setSlot] = useState("0");
  const [search, setSearch] = useState("");
  const [live, setLive] = useState<{
    mode: AutomationMode;
    role: string;
    roles: boolean;
  } | null>(null);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenSync: (() => void) | undefined;
    void (async () => {
      const { isTauri } = await import("@tauri-apps/api/core");
      if (!isTauri()) return;
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<{
        mode: AutomationMode;
        role: string;
        roles: boolean;
      }>("automation-context", ({ payload }) =>
        setLive((previous) =>
          previous?.mode === payload?.mode &&
          previous?.role === payload?.role &&
          previous?.roles === payload?.roles
            ? previous
            : payload,
        ),
      );
      if (disposed) stop();
      else unlisten = stop;
      const stopSync = await listen("cloud-sync-complete", async () => {
        if (dirty.current || disposed) return;
        const startedAt = revision.current;
        try {
          const api = await import("@/lib/tauri");
          const next = await api.getAutomationSettings();
          if (!disposed && !dirty.current && startedAt === revision.current) {
            latest.current = next;
            setSettings(next);
          }
        } catch {
          /* Keep the current editor if the client is temporarily unavailable. */
        }
      });
      if (disposed) stopSync();
      else unlistenSync = stopSync;
    })();
    return () => {
      disposed = true;
      unlisten?.();
      unlistenSync?.();
    };
  }, []);

  const load = useCallback(async () => {
    try {
      const api = await import("@/lib/tauri");
      const data = await api.getAutomationSettings();
      latest.current = data;
      setSettings(data);
      const [ver, champs, summoners, pages, paths] = await Promise.all([
        api.getDdragonVersion(),
        api.getChampions(),
        api.getSummonerSpells(),
        api.loadRunePages(),
        api.getRunePaths(),
      ]);
      setVersion(ver);
      setChampions(champs);
      setSpells(summoners);
      setRunePages(pages);
      setRunePaths(paths);
      setError("");
    } catch {
      setError("Не удалось загрузить настройки. Повторите попытку.");
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  const persist = useCallback((next: AutomationSettings) => {
    const currentRevision = ++revision.current;
    dirty.current = true;
    latest.current = next;
    setSettings(next);
    setSaving(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      timer.current = null;
      queue.current = queue.current.then(async () => {
        try {
          const api = await import("@/lib/tauri");
          const current = await api.getAutomationSettings();
          await api.setAutomationSettings({
            ...current,
            Profiles: next.Profiles,
            UseRoleProfiles: true,
            AutoPickEnabled: next.AutoPickEnabled,
            AutoBanEnabled: next.AutoBanEnabled,
          });
          if (revision.current === currentRevision) {
            dirty.current = false;
            setError("");
          }
        } catch {
          setError("Настройки не сохранены. Нажмите «Повторить».");
        } finally {
          if (revision.current === currentRevision) setSaving(false);
        }
      });
    }, 250);
  }, []);
  // Keep pending changes when leaving the page; the queued snapshot still reaches Tauri.
  const key = mode === "classic" ? `classic:${role}` : mode;
  const baseProfile = settings ? editorProfile(settings, key) : null;
  const isJungle = mode === "classic" && role === "jungle";
  const profile =
    baseProfile && isJungle ? jungleSpells(baseProfile) : baseProfile;
  const activePickCount = profile ? pickCount(profile, mode) : 3;
  const update = (patch: Partial<AutomationProfile>) => {
    const current = latest.current;
    if (!current) return;
    persist({
      ...current,
      Profiles: {
        ...current.Profiles,
        [key]: isJungle
          ? jungleSpells({ ...editorProfile(current, key), ...patch })
          : { ...editorProfile(current, key), ...patch },
      },
    });
  };
  const deleteRunePage = async (name: string) => {
    try {
      const api = await import("@/lib/tauri");
      await api.deleteRunePage(name);
      setRunePages(await api.loadRunePages());
      if (profile?.RunePage === name) update({ RunePage: "" });
    } catch {
      setError("Не удалось удалить страницу рун.");
    }
  };
  const canBan = mode === "classic" || mode === "arena";
  const canRunes = mode === "classic" || mode === "aram";
  const canSpells = mode !== "arena";
  const randomMode = mode === "aram" || mode === "mayhem";
  const spellChoices = Object.entries(spells).filter(([, id]) =>
    allowedSpells(mode).includes(Number(id)),
  );
  const filtered = useMemo(
    () =>
      Object.entries(champions)
        .filter(([name]) =>
          name.toLocaleLowerCase("ru").includes(search.toLocaleLowerCase("ru")),
        )
        .sort(([a], [b]) => a.localeCompare(b, "ru")),
    [champions, search],
  );

  const selectChampion = useCallback(
    (name: string) => {
      const current = latest.current;
      if (!current) return;
      const selectedProfile = editorProfile(current, key);
      const next = { ...selectedProfile };
      if (slot === "ban") next.Ban = name;
      else {
        const picks = [...selectedProfile.Picks];
        const index = Number(slot);
        const duplicate = picks.indexOf(name);
        if (duplicate >= 0 && duplicate !== index)
          picks[duplicate] = picks[index] || "";
        picks[index] = name;
        next.Picks = picks;
        if (mode === "aram" || mode === "mayhem") {
          const count = pickCount(next, mode);
          const empty = Array.from({ length: count }, (_, i) => i).find(
            (i) => !picks[i],
          );
          if (empty != null) setSlot(String(empty));
        }
      }
      persist({ ...current, Profiles: { ...current.Profiles, [key]: next } });
    },
    [key, slot, mode, persist],
  );
  const saveRunePage = useCallback(async (page: RunePage) => {
    const api = await import("@/lib/tauri");
    await api.saveRunePage(page);
    setRunePages(await api.loadRunePages());
  }, []);
  if (!settings || !profile)
    return (
      <div className="flex flex-col gap-3">
        <h1 className="text-lg font-semibold">Автоматизация</h1>
        {error ? (
          <>
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
            <Button size="sm" variant="outline" onClick={() => void load()}>
              Повторить
            </Button>
          </>
        ) : (
          <Skeleton className="h-60 w-full" />
        )}
      </div>
    );
  const toggle = (
    label: string,
    field: "AutoPick" | "AutoBan" | "AutoSpells" | "AutoRunes" | "Bench",
    disabled = false,
  ) => (
    <label className="flex items-center gap-2 text-xs">
      <Switch
        size="sm"
        aria-label={label}
        checked={
          !disabled &&
          (mode === "classic" && field === "AutoPick"
            ? (settings.AutoPickEnabled ?? true)
            : mode === "classic" && field === "AutoBan"
              ? (settings.AutoBanEnabled ?? true)
              : profile[field])
        }
        disabled={disabled}
        onCheckedChange={(value) => {
          if (
            mode === "classic" &&
            (field === "AutoPick" || field === "AutoBan")
          )
            persist({
              ...latest.current!,
              [field === "AutoPick" ? "AutoPickEnabled" : "AutoBanEnabled"]:
                value,
            });
          else update({ [field]: value });
        }}
      />
      {label}
    </label>
  );

  return (
    <div className="accounts-page flex flex-col gap-3">
      {live && (
        <Badge variant="outline" className="w-fit">
          Клиент: {MODES[live.mode]}
          {live.mode === "classic" && live.roles
            ? ` · ${ROLES[live.role as keyof typeof ROLES] || "Роль не определена"}`
            : ""}
        </Badge>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-lg font-semibold">Автоматизация</h1>
        <AutoAcceptSwitch />
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div
          className="flex flex-wrap gap-2"
          role="group"
          aria-label="Режим игры"
        >
          {Object.entries(MODES).map(([value, label]) => (
            <Button
              key={value}
              size="sm"
              variant={mode === value ? "default" : "outline"}
              aria-pressed={mode === value}
              onClick={() => {
                setMode(value as AutomationMode);
                setSlot("0");
              }}
            >
              {label}
            </Button>
          ))}
        </div>
        <span role="status" className="text-xs text-muted-foreground">
          {saving ? "Сохраняем…" : error ? "Не сохранено" : "Сохранено"}
        </span>
      </div>
      {error && (
        <div
          role="alert"
          className="flex items-center gap-2 text-xs text-destructive"
        >
          {error}
          <Button
            size="sm"
            variant="outline"
            onClick={() => latest.current && persist(latest.current)}
          >
            Повторить
          </Button>
        </div>
      )}
      {mode === "classic" && (
        <div className="flex flex-wrap gap-2" role="group" aria-label="Роль">
          {Object.entries(ROLES).map(([value, label]) => (
            <Button
              key={value}
              size="sm"
              variant={role === value ? "default" : "outline"}
              aria-pressed={role === value}
              onClick={() => setRole(value)}
            >
              <LolRoleIcon role={value as keyof typeof ROLES} />
              {label}
            </Button>
          ))}
        </div>
      )}
      <Card className="gap-3 py-3">
        <CardHeader className="px-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <CardTitle className="text-sm">Чемпионы</CardTitle>
            <div className="flex gap-3">
              {toggle(
                "Автопик",
                "AutoPick",
                mode === "arena" && profile.ArenaPick === "manual",
              )}
              {canBan && toggle("Автобан", "AutoBan")}
            </div>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 px-3">
          {mode === "arena" && (
            <Select
              value={profile.ArenaPick}
              onValueChange={(value) =>
                value &&
                update({ ArenaPick: value as AutomationProfile["ArenaPick"] })
              }
            >
              <SelectTrigger size="sm" aria-label="Выбор на Арене">
                <SelectValue>
                  {profile.ArenaPick === "crowd"
                    ? "За наковальню"
                    : profile.ArenaPick === "manual"
                      ? "Вручную / Bravery"
                      : "По приоритету"}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="preferred">По приоритету</SelectItem>
                  <SelectItem value="crowd">За наковальню</SelectItem>
                  <SelectItem value="manual">Вручную / Bravery</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          )}
          {randomMode && (
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted-foreground">Пиков</span>
              <Select
                value={String(activePickCount)}
                onValueChange={(value) => {
                  if (value) {
                    const count = Number(value);
                    update({ PickCount: count });
                    if (slot !== "ban" && Number(slot) >= count) setSlot("0");
                  }
                }}
              >
                <SelectTrigger
                  size="sm"
                  aria-label="Количество приоритетов ARAM"
                >
                  <SelectValue>{activePickCount}</SelectValue>
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {Array.from({ length: 30 }, (_, i) => (
                      <SelectItem key={i + 1} value={String(i + 1)}>
                        {i + 1}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
          )}
          <Tabs
            value={slot}
            onValueChange={(value) => value && setSlot(String(value))}
          >
            <TabsList className="flex h-auto max-h-24 flex-wrap justify-start overflow-y-auto">
              {Array.from(
                { length: randomMode ? activePickCount : 3 },
                (_, index) => (
                  <TabsTrigger
                    key={index}
                    value={String(index)}
                    aria-label={`Приоритет пика ${index + 1}`}
                  >
                    {profile.Picks[index] ||
                      (randomMode
                        ? "+"
                        : ["Пик", "Запасной пик", "Резервный пик"][index])}
                  </TabsTrigger>
                ),
              )}
              {canBan && (
                <TabsTrigger value="ban">Бан: {profile.Ban || "—"}</TabsTrigger>
              )}
            </TabsList>
          </Tabs>
          <div className="flex items-center gap-2">
            <div className="relative w-48">
              <Search className="absolute left-2 top-2 size-3.5 text-muted-foreground" />
              <Input
                className="h-7 pl-7"
                placeholder="Поиск чемпиона"
                aria-label="Поиск чемпиона"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
            </div>
            <WithTooltip label="Сбросить выбранный слот">
              <Button
                size="icon-sm"
                variant="outline"
                aria-label="Сбросить выбранный слот"
                onClick={() => {
                  if (slot === "ban") update({ Ban: "" });
                  else {
                    const picks = [...profile.Picks];
                    picks[Number(slot)] = "";
                    update({ Picks: picks });
                  }
                }}
              >
                <X />
              </Button>
            </WithTooltip>
          </div>
          <ChampionGrid
            entries={filtered}
            version={version}
            selected={
              slot === "ban" ? profile.Ban : profile.Picks[Number(slot)] || ""
            }
            onSelect={selectChampion}
          />
          {randomMode && toggle("Брать со скамейки", "Bench")}
        </CardContent>
      </Card>
      {canSpells && (
        <>
          {/* Summoner spells */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base flex items-center justify-between">
                <span>Заклинания призывателя</span>
                <Switch
                  aria-label="Автовыбор заклинаний"
                  checked={profile.AutoSpells}
                  onCheckedChange={(v) => update({ AutoSpells: v })}
                />
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex flex-wrap gap-1.5">
                {spellChoices.map(([name, key]) => {
                  const imgName = SPELL_IMAGE_MAP[key] || "SummonerFlash";
                  const isSpell1 = profile.Spell1Id === Number(key);
                  const isSpell2 = profile.Spell2Id === Number(key);
                  return (
                    <WithTooltip key={key} label={name}>
                      <button
                        type="button"
                        aria-label={`Заклинание: ${name}`}
                        aria-pressed={isSpell1 || isSpell2}
                        data-selected={isSpell1 || isSpell2 || undefined}
                        disabled={isJungle && Number(key) === 11}
                        onClick={() => {
                          const id = Number(key);
                          if (isJungle)
                            update(
                              profile.Spell1Id === 11
                                ? { Spell2Id: id }
                                : { Spell1Id: id },
                            );
                          else if (!profile.Spell1Id || profile.Spell2Id)
                            update({ Spell1Id: id, Spell2Id: null });
                          else if (id !== profile.Spell1Id)
                            update({ Spell2Id: id });
                        }}
                        className={`lm-image-choice rounded-md overflow-hidden border-2 transition-colors w-10 h-10 ${
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
                {isJungle && (
                  <WithTooltip label="Поменять D и F">
                    <Button
                      size="icon-sm"
                      variant="outline"
                      aria-label="Поменять заклинания D и F"
                      onClick={() =>
                        update({
                          Spell1Id: profile.Spell2Id,
                          Spell2Id: profile.Spell1Id,
                        })
                      }
                    >
                      <ArrowLeftRight />
                    </Button>
                  </WithTooltip>
                )}
                <div className="inline-flex items-stretch overflow-hidden rounded-md border border-border bg-muted/40">
                  <span className="flex items-center px-2.5 py-1 font-mono text-xs font-bold tabular-nums text-muted-foreground">
                    D
                  </span>
                  {profile.Spell1Id ? (
                    <>
                      <div
                        className="w-px shrink-0 self-stretch bg-border"
                        aria-hidden
                      />
                      <div className="flex items-center gap-1.5 px-2.5 py-1">
                        <span className="text-foreground">
                          {Object.entries(spells).find(
                            ([, k]) => Number(k) === profile.Spell1Id,
                          )?.[0] || "—"}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-4 w-4 shrink-0 p-0 text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
                          aria-label="Сбросить заклинание D"
                          disabled={isJungle && profile.Spell1Id === 11}
                          onClick={() => update({ Spell1Id: null })}
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
                  {profile.Spell2Id ? (
                    <>
                      <div
                        className="w-px shrink-0 self-stretch bg-border"
                        aria-hidden
                      />
                      <div className="flex items-center gap-1.5 px-2.5 py-1">
                        <span className="text-foreground">
                          {Object.entries(spells).find(
                            ([, k]) => Number(k) === profile.Spell2Id,
                          )?.[0] || "—"}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-4 w-4 shrink-0 p-0 text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
                          aria-label="Сбросить заклинание F"
                          disabled={isJungle && profile.Spell2Id === 11}
                          onClick={() => update({ Spell2Id: null })}
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
        </>
      )}
      {canRunes && (
        <>
          {/* Rune pages */}
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <div className="flex items-center gap-3">
                <CardTitle className="text-base">Страницы рун</CardTitle>
                <Switch
                  aria-label="Автовыбор рун"
                  checked={profile.AutoRunes}
                  onCheckedChange={(v) => update({ AutoRunes: v })}
                />
              </div>
              <div className="flex items-center gap-2">
                <span className="text-sm text-muted-foreground">
                  {runePages.length} сохранено
                </span>
                <Button
                  size="sm"
                  onClick={() => {
                    setEditingRunePage(null);
                    setRuneEditorOpen(true);
                  }}
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
                    const primaryPath = runePaths.find(
                      (p) => p.id === page.PrimaryPathId,
                    );
                    const secondaryPath = runePaths.find(
                      (p) => p.id === page.SecondaryPathId,
                    );
                    const isSelected = profile.RunePage === page.Name;
                    return (
                      <div
                        key={page.Name}
                        role="button"
                        tabIndex={0}
                        aria-label={`Выбрать руны: ${page.Name}`}
                        aria-pressed={isSelected}
                        onKeyDown={(event) => {
                          if (
                            event.target === event.currentTarget &&
                            (event.key === "Enter" || event.key === " ")
                          ) {
                            event.preventDefault();
                            update({ RunePage: page.Name });
                          }
                        }}
                        className={`flex items-center justify-between rounded-lg border p-3 cursor-pointer transition-colors ${
                          isSelected
                            ? "border-primary bg-primary/5"
                            : "border-border hover:border-muted-foreground/30"
                        }`}
                        onClick={() => update({ RunePage: page.Name })}
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
                          <span className="text-sm font-medium">
                            {page.Name}
                          </span>
                          {isSelected && (
                            <Badge variant="secondary" className="text-xs">
                              Выбрано
                            </Badge>
                          )}
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
                              void deleteRunePage(page.Name);
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
        </>
      )}
      {runeEditorOpen && (
        <RunePageEditor
          open={runeEditorOpen}
          onOpenChange={setRuneEditorOpen}
          runePaths={runePaths}
          editPage={editingRunePage}
          onSave={saveRunePage}
        />
      )}
    </div>
  );
}
