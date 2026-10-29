"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { X, Plus, Pencil, Trash2, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
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
  type AutomationMode,
} from "@/lib/automation";
import { cn } from "@/lib/utils";
import type {
  RunePage,
  RunePathModel,
  AutomationSettings,
  AutomationProfile,
} from "@/lib/tauri";

const DDRAGON = "https://ddragon.leagueoflegends.com";
const MODE_HELP = {
  classic:
    "Профиль выбирается по назначенной роли; незаполненная роль остаётся под вашим управлением.",
  aram: "Приоритеты применяются к доступным чемпионам; скамейка — после открытия общего выбора.",
  mayhem: "Выбор как в ARAM; руны и изнурение отключены правилами режима.",
  arena:
    "Пик и бан отдельно; руны и заклинания задаёт игра, Bravery выбирается вручную.",
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
      }>("automation-context", ({ payload }) => setLive(payload));
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
            UseRoleProfiles: next.UseRoleProfiles,
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
  const key =
    mode === "classic" && settings?.UseRoleProfiles ? `classic:${role}` : mode;
  const profile = settings ? editorProfile(settings, key) : null;
  const update = (patch: Partial<AutomationProfile>) => {
    const current = latest.current;
    if (!current) return;
    persist({
      ...current,
      Profiles: {
        ...current.Profiles,
        [key]: { ...editorProfile(current, key), ...patch },
      },
    });
  };
  const canBan = mode === "classic" || mode === "arena";
  const canRunes = mode === "classic" || mode === "aram";
  const canSpells = mode !== "arena";
  const randomMode = mode === "aram" || mode === "mayhem";
  const spellChoices = Object.entries(spells).filter(([, id]) =>
    allowedSpells(mode).includes(Number(id)),
  );
  const filtered = Object.entries(champions)
    .filter(([name]) =>
      name.toLocaleLowerCase("ru").includes(search.toLocaleLowerCase("ru")),
    )
    .sort(([a], [b]) => a.localeCompare(b, "ru"));

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
        checked={!disabled && profile[field]}
        disabled={disabled}
        onCheckedChange={(value) => update({ [field]: value })}
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
        <Tabs
          value={mode}
          onValueChange={(value) => {
            if (value && value in MODES) {
              setMode(value as AutomationMode);
              setSlot("0");
            }
          }}
        >
          <TabsList>
            {Object.entries(MODES).map(([value, label]) => (
              <TabsTrigger value={value} key={value}>
                {label}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
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
      <p className="text-xs text-muted-foreground">{MODE_HELP[mode]}</p>
      {mode === "classic" && (
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 text-xs">
            <Switch
              size="sm"
              checked={Boolean(settings.UseRoleProfiles)}
              onCheckedChange={(value) =>
                persist({ ...latest.current!, UseRoleProfiles: value })
              }
            />
            По ролям
          </label>
          {settings.UseRoleProfiles && (
            <Tabs
              value={role}
              onValueChange={(value) => value && setRole(String(value))}
            >
              <TabsList>
                {Object.entries(ROLES).map(([value, label]) => (
                  <TabsTrigger value={value} key={value}>
                    {label}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          )}
          {!settings.UseRoleProfiles && (
            <Badge variant="outline">Общий профиль</Badge>
          )}
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
          <Tabs
            value={slot}
            onValueChange={(value) => value && setSlot(String(value))}
          >
            <TabsList className="flex h-auto flex-wrap justify-start">
              {[0, 1, 2].map((index) => (
                <TabsTrigger key={index} value={String(index)}>
                  {index + 1}. {profile.Picks[index] || "Пик"}
                </TabsTrigger>
              ))}
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
          <div className="grid max-h-48 grid-cols-[repeat(auto-fill,2.5rem)] justify-start gap-1.5 overflow-y-auto rounded-md border p-2">
            {filtered.map(([name, english]) => (
              <WithTooltip key={english} label={name}>
                <button
                  type="button"
                  aria-label={name}
                  aria-pressed={
                    slot === "ban"
                      ? profile.Ban === name
                      : profile.Picks[Number(slot)] === name
                  }
                  className={cn(
                    "size-10 overflow-hidden rounded-md border-2 border-transparent",
                    (slot === "ban"
                      ? profile.Ban === name
                      : profile.Picks[Number(slot)] === name) &&
                      "border-primary",
                  )}
                  onClick={() => {
                    if (slot === "ban") update({ Ban: name });
                    else {
                      const picks = [...profile.Picks];
                      picks[Number(slot)] = name;
                      update({ Picks: picks });
                    }
                  }}
                >
                  <img
                    src={`${DDRAGON}/cdn/${version}/img/champion/${english}.png`}
                    alt=""
                    loading="lazy"
                    className="size-full"
                  />
                </button>
              </WithTooltip>
            ))}
          </div>
          {randomMode && toggle("Брать со скамейки", "Bench")}
          <CardDescription className="text-xs">
            {randomMode
              ? "Нет нужного чемпиона — LM оставит текущего; баны не выполняются."
              : "Пики слева направо; недоступные чемпионы пропускаются."}
          </CardDescription>
        </CardContent>
      </Card>
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <Card className="gap-3 py-3">
          <CardHeader className="px-3">
            <CardTitle className="text-sm">Заклинания</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-3 px-3">
            {canSpells ? (
              <>
                {toggle("Автозаклинания", "AutoSpells")}
                <div className="flex flex-wrap gap-2">
                  {(["Spell1Id", "Spell2Id"] as const).map((field, index) => (
                    <Select
                      key={field}
                      value={profile[field] ? String(profile[field]) : "none"}
                      onValueChange={(value) => {
                        if (value)
                          update({
                            [field]: value === "none" ? null : Number(value),
                          });
                      }}
                    >
                      <SelectTrigger
                        size="sm"
                        aria-label={`Заклинание ${index ? "F" : "D"}`}
                      >
                        <SelectValue>
                          {index ? "F" : "D"}:{" "}
                          {spellChoices.find(
                            ([, id]) => Number(id) === profile[field],
                          )?.[0] || "Не менять"}
                        </SelectValue>
                      </SelectTrigger>
                      <SelectContent>
                        <SelectGroup>
                          <SelectItem value="none">Не менять</SelectItem>
                          {spellChoices.map(([name, id]) => (
                            <SelectItem
                              key={id}
                              value={id}
                              disabled={
                                Number(id) ===
                                profile[index ? "Spell1Id" : "Spell2Id"]
                              }
                            >
                              {name}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  ))}
                </div>
                <p className="text-xs text-muted-foreground">
                  Требования роли и ограничения клиента имеют приоритет.
                </p>
              </>
            ) : (
              <p className="text-xs text-muted-foreground">
                Фиксированы игрой: Скачок и Побег.
              </p>
            )}
          </CardContent>
        </Card>
        <Card className="gap-3 py-3">
          <CardHeader className="px-3">
            <CardTitle className="text-sm">Руны</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-3 px-3">
            {canRunes ? (
              <>
                {toggle("Авторуны", "AutoRunes")}
                <Select
                  value={profile.RunePage || "none"}
                  onValueChange={(value) =>
                    value && update({ RunePage: value === "none" ? "" : value })
                  }
                >
                  <SelectTrigger size="sm" aria-label="Страница рун">
                    <SelectValue>{profile.RunePage || "Не менять"}</SelectValue>
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="none">Не менять</SelectItem>
                      {runePages.map((page) => (
                        <SelectItem key={page.Name} value={page.Name}>
                          {page.Name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
                <div className="flex flex-wrap gap-1">
                  <WithTooltip label="Создать страницу">
                    <Button
                      size="icon-sm"
                      variant="outline"
                      aria-label="Создать страницу рун"
                      onClick={() => {
                        setEditingRunePage(null);
                        setRuneEditorOpen(true);
                      }}
                    >
                      <Plus />
                    </Button>
                  </WithTooltip>
                  <WithTooltip label="Изменить страницу">
                    <Button
                      size="icon-sm"
                      variant="outline"
                      aria-label="Изменить страницу рун"
                      disabled={
                        !runePages.some((p) => p.Name === profile.RunePage)
                      }
                      onClick={() => {
                        setEditingRunePage(
                          runePages.find((p) => p.Name === profile.RunePage) ||
                            null,
                        );
                        setRuneEditorOpen(true);
                      }}
                    >
                      <Pencil />
                    </Button>
                  </WithTooltip>
                  <WithTooltip label="Удалить страницу">
                    <Button
                      size="icon-sm"
                      variant="outline"
                      aria-label="Удалить страницу рун"
                      disabled={
                        !runePages.some((p) => p.Name === profile.RunePage)
                      }
                      onClick={async () => {
                        try {
                          const api = await import("@/lib/tauri");
                          await api.deleteRunePage(profile.RunePage);
                          setRunePages(await api.loadRunePages());
                          update({ RunePage: "" });
                        } catch {
                          setError("Не удалось удалить страницу рун.");
                        }
                      }}
                    >
                      <Trash2 />
                    </Button>
                  </WithTooltip>
                </div>
              </>
            ) : (
              <p className="text-xs text-muted-foreground">
                Отключены правилами режима.
              </p>
            )}
          </CardContent>
        </Card>
      </div>
      <RunePageEditor
        open={runeEditorOpen}
        onOpenChange={setRuneEditorOpen}
        runePaths={runePaths}
        editPage={editingRunePage}
        onSave={async (page) => {
          try {
            const api = await import("@/lib/tauri");
            await api.saveRunePage(page);
            setRunePages(await api.loadRunePages());
          } catch {
            setError("Не удалось сохранить страницу рун.");
          }
        }}
      />
    </div>
  );
}
