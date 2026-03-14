"use client";

import { useState, useCallback, useEffect } from "react";
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
import type { PlayerInfo } from "@/lib/tauri";

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

function PlayerCard({ player }: { player: PlayerInfo }) {
  const tierColor: Record<string, string> = {
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

  return (
    <div className="flex items-center justify-between rounded-lg border border-border p-3">
      <div className="flex items-center gap-3">
        <div>
          <div className="text-sm font-medium">{player.riot_id || player.summoner_name}</div>
          <div className="text-xs text-muted-foreground">
            Lvl {player.level}
          </div>
        </div>
      </div>
      <div className="text-right">
        <div className={`text-sm font-medium ${tierColor[player.tier] || "text-muted-foreground"}`}>
          {player.tier !== "Unranked"
            ? `${player.tier} ${player.rank} ${player.league_points}LP`
            : "Unranked"}
        </div>
        <div className="text-xs text-muted-foreground">
          {player.wins}W {player.losses}L ({player.win_rate})
        </div>
      </div>
    </div>
  );
}

export default function SpyPage() {
  const [apiKey, setApiKey] = useState("");
  const [region, setRegion] = useState("euw1");
  const [allies, setAllies] = useState<PlayerInfo[]>([]);
  const [enemies, setEnemies] = useState<PlayerInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [configured, setConfigured] = useState(false);
  const [apiStatus, setApiStatus] = useState<string | null>(null);

  useEffect(() => {
    import("@/lib/tauri").then(({ getRevealApiConfig }) => {
      getRevealApiConfig().then(([savedKey, savedRegion]) => {
        if (savedKey) {
          setApiKey(savedKey);
          setConfigured(true);
        }
        if (savedRegion) setRegion(savedRegion);
      }).catch(() => {});
    });
  }, []);

  const handleSaveConfig = async () => {
    try {
      const { setRevealApiConfig, testApiKey } = await import("@/lib/tauri");
      await setRevealApiConfig(apiKey, region);
      const [valid, msg] = await testApiKey();
      setApiStatus(valid ? "API ключ валиден" : `Ошибка: ${msg}`);
      setConfigured(valid);
    } catch (e) {
      setApiStatus(`Ошибка: ${e}`);
    }
  };

  const handleRefresh = useCallback(async () => {
    setLoading(true);
    try {
      const { getTeamsInfo } = await import("@/lib/tauri");
      const [a, e] = await getTeamsInfo();
      setAllies(a);
      setEnemies(e);
    } catch (e) {
      console.error("Refresh failed:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Разведка</h1>
        {configured && (
          <Button onClick={handleRefresh} disabled={loading}>
            {loading ? "Загрузка..." : "Обновить"}
          </Button>
        )}
      </div>

      <div className="rounded-xl border border-destructive/50 bg-destructive/10 p-4">
        <p className="text-sm text-destructive">
          Эта функция использует Riot API. Необходим API ключ с{" "}
          <span className="font-medium">developer.riotgames.com</span>.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Настройки API</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
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
            <Button onClick={handleSaveConfig}>Сохранить</Button>
          </div>
          {apiStatus && (
            <Badge variant={configured ? "secondary" : "destructive"}>
              {apiStatus}
            </Badge>
          )}
        </CardContent>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">
              Союзники ({allies.length})
            </CardTitle>
          </CardHeader>
          <CardContent>
            {allies.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">
                {configured ? "Войдите в выбор чемпионов" : "Настройте API ключ"}
              </p>
            ) : (
              <div className="space-y-2">
                {allies.map((p, i) => (
                  <PlayerCard key={i} player={p} />
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">
              Противники ({enemies.length})
            </CardTitle>
          </CardHeader>
          <CardContent>
            {enemies.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">
                {configured ? "Войдите в выбор чемпионов" : "Настройте API ключ"}
              </p>
            ) : (
              <div className="space-y-2">
                {enemies.map((p, i) => (
                  <PlayerCard key={i} player={p} />
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
