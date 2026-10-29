"use client";

import { useState, useCallback, useEffect, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import type { PlayerInfo, RevealSnapshot } from "@/lib/tauri";
import { championIconUrl, positionLabel, profileIconUrl } from "@/lib/lol-assets";

const REGIONS = [
  { code: "euw1", name: "EU West" },
  { code: "eun1", name: "EU Nordic & East" },
  { code: "na1", name: "North America" },
  { code: "kr", name: "Korea" },
  { code: "ru", name: "Russia" },
  { code: "tr1", name: "Turkey" },
  { code: "br1", name: "Brazil" },
  { code: "jp1", name: "Japan" },
  { code: "la1", name: "Latin America North" },
  { code: "la2", name: "Latin America South" },
  { code: "oc1", name: "Oceania" },
];

const TIER_CLASSES: Record<string, string> = {
  IRON: "text-gray-400",
  BRONZE: "text-amber-700",
  SILVER: "text-gray-300",
  GOLD: "text-yellow-500",
  PLATINUM: "text-teal-400",
  EMERALD: "text-emerald-400",
  DIAMOND: "text-blue-400",
  MASTER: "text-purple-400",
  GRANDMASTER: "text-red-500",
  CHALLENGER: "text-cyan-300",
};

function PlayerRow({
  player,
  fallbackLabel,
}: {
  player: PlayerInfo;
  fallbackLabel: string;
}) {
  const [iconFailed, setIconFailed] = useState(false);
  const icon = championIconUrl(player.champion_id) ?? profileIconUrl(player.profile_icon_id);
  const name = player.riot_id || player.summoner_name;
  const position = positionLabel(player.position);

  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border border-border p-3">
      <div className="flex min-w-0 items-center gap-3">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
          {icon && !iconFailed ? (
            <img
              src={icon}
              alt=""
              className="h-10 w-10 object-cover"
              onError={() => setIconFailed(true)}
            />
          ) : (
            <span className="text-xs text-muted-foreground">?</span>
          )}
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className={`truncate text-xs font-medium ${name ? "" : "italic text-muted-foreground"}`}>
              {name || "Ник скрыт"}
            </span>
            {player.is_self && (
              <Badge variant="secondary" className="shrink-0">
                Вы
              </Badge>
            )}
          </div>
          <div className="truncate text-xs text-muted-foreground">
            {position || fallbackLabel}
            {player.level > 0 ? ` · Lvl ${player.level}` : ""}
            {player.champion_id > 0 ? "" : " · пик не выбран"}
          </div>
        </div>
      </div>
      <div className="shrink-0 text-right">
        <div
          className={`text-xs font-medium ${
            TIER_CLASSES[player.tier] || "text-muted-foreground"
          }`}
        >
          {player.tier && player.tier !== "Unranked"
            ? `${player.tier} ${player.rank} ${player.league_points}LP`
            : "Unranked"}
        </div>
        <div className="text-xs text-muted-foreground">
          {player.wins + player.losses > 0
            ? `${player.wins}W ${player.losses}L (${player.win_rate})`
            : "нет данных по рангам"}
        </div>
      </div>
    </div>
  );
}

export default function SpyPage() {
  const [apiKey, setApiKey] = useState("");
  const [region, setRegion] = useState("euw1");
  const [snapshot, setSnapshot] = useState<RevealSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [apiStatus, setApiStatus] = useState<string | null>(null);
  const [isDesktop, setIsDesktop] = useState(false);
  const inFlight = useRef(false);

  const loadRevealConfig = useCallback(() => {
    import("@/lib/tauri").then(({ getRevealApiConfig }) => {
      getRevealApiConfig()
        .then(([savedKey, savedRegion]) => {
          setApiKey(savedKey ?? "");
          if (savedRegion) setRegion(savedRegion);
        })
        .catch(() => {});
    });
  }, []);

  useEffect(() => {
    loadRevealConfig();
  }, [loadRevealConfig]);

  useEffect(() => {
    let disposed = false;
    let un: (() => void) | undefined;
    void (async () => {
      const { isTauri } = await import("@tauri-apps/api/core");
      setIsDesktop(isTauri());
      if (!isTauri()) return;
      const { listen } = await import("@tauri-apps/api/event");
      un = await listen("cloud-sync-complete", () => {
        loadRevealConfig();
      });
      if (disposed) un();
    })();
    return () => {
      disposed = true;
      un?.();
    };
  }, [loadRevealConfig]);

  const refresh = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setLoading(true);
    try {
      const { getTeamsInfo } = await import("@/lib/tauri");
      const next = await getTeamsInfo();
      setSnapshot(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      inFlight.current = false;
      setLoading(false);
    }
  }, []);

  // Разведка живёт своей жизнью: клиент меняет состав каждые пару секунд,
  // кнопка «Обновить» для этого не нужна.
  useEffect(() => {
    if (!isDesktop) return;
    void refresh();
    const timer = setInterval(() => {
      if (!document.hidden) void refresh();
    }, 3000);
    return () => clearInterval(timer);
  }, [isDesktop, refresh]);

  const handleSaveConfig = async () => {
    try {
      const { setRevealApiConfig, testApiKey } = await import("@/lib/tauri");
      await setRevealApiConfig(apiKey, region);
      const [valid, msg] = await testApiKey();
      setApiStatus(valid ? "API ключ валиден" : `Ошибка: ${msg}`);
      await refresh();
    } catch (e) {
      setApiStatus(`Ошибка: ${e}`);
    }
  };

  const inChampSelect = snapshot?.in_champ_select ?? false;
  const allies = snapshot?.allies ?? [];
  const enemies = snapshot?.enemies ?? [];
  const noApiKey = snapshot ? !snapshot.api_configured : false;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <h1 className="text-lg font-semibold">Разведка</h1>
          {snapshot?.queue_name && <Badge variant="secondary">{snapshot.queue_name}</Badge>}
          {inChampSelect && (
            <span className="text-xs text-muted-foreground">обновляется автоматически</span>
          )}
        </div>
        <Button onClick={() => void refresh()} disabled={loading || !isDesktop}>
          {loading ? "Обновление..." : "Обновить"}
        </Button>
      </div>

      {!isDesktop && (
        <div className="rounded-xl border border-border bg-muted/30 p-4 text-xs text-muted-foreground">
          Разведка работает только в собранном приложении: нужен запущенный клиент League of Legends.
        </div>
      )}

      {error && (
        <div className="rounded-xl border border-destructive/50 bg-destructive/10 p-4 text-xs text-destructive">
          {error}
        </div>
      )}

      {snapshot?.note && (
        <div className="rounded-xl border border-border bg-muted/30 p-4 text-xs text-muted-foreground">
          {snapshot.note}
        </div>
      )}

      {snapshot && snapshot.unmatched_names.length > 0 && (
        <div className="rounded-xl border border-amber-500/40 bg-amber-500/5 p-3 text-xs text-amber-600 dark:text-amber-400">
          Клиент отдал ники, но не привязал их к слотам: {snapshot.unmatched_names.join(", ")}
        </div>
      )}

      {snapshot?.api_error && (
        <div className="rounded-xl border border-amber-500/50 bg-amber-500/10 p-4 text-xs text-amber-600 dark:text-amber-400">
          {snapshot.api_error}
        </div>
      )}

      <div className="grid gap-3 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Союзники ({allies.length})</CardTitle>
          </CardHeader>
          <CardContent>
            {allies.length === 0 ? (
              <p className="py-4 text-center text-xs text-muted-foreground">
                {inChampSelect
                  ? "Клиент не отдал состав команды."
                  : "Войди в выбор чемпионов — данные появятся автоматически."}
              </p>
            ) : (
              <div className="space-y-2">
                {allies.map((player) => (
                  <PlayerRow
                    key={`ally-${player.cell_id}`}
                    player={player}
                    fallbackLabel={`Союзник ${player.cell_id + 1}`}
                  />
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Противники ({enemies.length})</CardTitle>
          </CardHeader>
          <CardContent>
            {enemies.length === 0 ? (
              <p className="py-4 text-center text-xs text-muted-foreground">
                {inChampSelect
                  ? "Riot не показывает противников в выборе чемпионов — они видны только в кастомных лобби."
                  : "Войди в выбор чемпионов — данные появятся автоматически."}
              </p>
            ) : (
              <div className="space-y-2">
                {enemies.map((player) => (
                  <PlayerRow
                    key={`enemy-${player.cell_id}`}
                    player={player}
                    fallbackLabel={`Противник ${player.cell_id - 4}`}
                  />
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">
            Riot API {noApiKey ? "— ключ не задан" : ""}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-xs text-muted-foreground">
            Ключ нужен только для рангов и винрейта: ники, позиции и чемпионы читаются из
            клиента. Dev-ключ Riot живёт 24 часа — просроченный подсвечивается выше.
          </p>
          <div className="flex gap-3">
            <Input
              placeholder="RGAPI-xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              type="password"
              className="flex-1"
            />
            <Select value={region} onValueChange={(v) => v && setRegion(v)}>
              <SelectTrigger className="w-[180px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {REGIONS.map((r) => (
                  <SelectItem key={r.code} value={r.code}>
                    {r.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button onClick={() => void handleSaveConfig()}>Сохранить</Button>
          </div>
          {apiStatus && (
            <Badge variant={apiStatus.startsWith("Ошибка") ? "destructive" : "secondary"}>
              {apiStatus}
            </Badge>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
