"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const SERVERS = [
  { code: "EUW", name: "EU West" },
  { code: "EUNE", name: "EU Nordic & East" },
  { code: "NA", name: "North America" },
  { code: "KR", name: "Korea" },
  { code: "RU", name: "Russia" },
  { code: "TR", name: "Turkey" },
  { code: "BR", name: "Brazil" },
  { code: "JP", name: "Japan" },
  { code: "LAN", name: "Latin America North" },
  { code: "LAS", name: "Latin America South" },
  { code: "OCE", name: "Oceania" },
];

export default function AddAccountPage() {
  const router = useRouter();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [note, setNote] = useState("");
  const [server, setServer] = useState("");
  const [saving, setSaving] = useState(false);

  const handleSubmit = async () => {
    if (!username || !password) return;
    setSaving(true);
    try {
      const { protectPassword, saveAccount } = await import("@/lib/tauri");
      const encryptedPassword = await protectPassword(password);
      await saveAccount({
        Username: username,
        EncryptedPassword: encryptedPassword,
        Note: note,
        CreatedAt: new Date().toISOString(),
        AvatarUrl: "",
        SummonerName: "",
        Rank: "",
        RankDisplay: "",
        RiotId: "",
        RankIconUrl: "",
        Server: server,
      });
      router.push("/accounts");
    } catch (e) {
      console.error("Save failed:", e);
    } finally {
      setSaving(false);
    }
  };

  const handleClear = () => {
    setUsername("");
    setPassword("");
    setNote("");
    setServer("");
  };

  return (
    <div className="max-w-md space-y-6">
      <h1 className="text-2xl font-bold">Добавить аккаунт</h1>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">Данные аккаунта</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <label className="text-sm text-muted-foreground">Логин</label>
            <Input
              type="text"
              placeholder="Введите логин"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm text-muted-foreground">Пароль</label>
            <Input
              type="password"
              placeholder="Введите пароль"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm text-muted-foreground">Сервер</label>
            <Select value={server} onValueChange={(v) => v && setServer(v)}>
              <SelectTrigger>
                <SelectValue placeholder="Выберите сервер" />
              </SelectTrigger>
              <SelectContent>
                {SERVERS.map((s) => (
                  <SelectItem key={s.code} value={s.code}>
                    {s.code} — {s.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <label className="text-sm text-muted-foreground">Заметка</label>
            <Input
              placeholder="Необязательная заметка"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </div>
          <div className="flex gap-2 pt-2">
            <Button onClick={handleSubmit} disabled={saving || !username || !password}>
              {saving ? "Сохранение..." : "Добавить"}
            </Button>
            <Button variant="secondary" onClick={handleClear}>
              Очистить
            </Button>
            <Button variant="outline" onClick={() => router.push("/accounts")}>
              Назад
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
