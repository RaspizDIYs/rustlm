"use client";

import { useState } from "react";
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

export default function CustomizationPage() {
  const [status, setStatus] = useState("");
  const [availability, setAvailability] = useState("chat");
  const [iconId, setIconId] = useState("");
  const [skinId, setSkinId] = useState("");
  const [result, setResult] = useState<string | null>(null);

  const handleSetStatus = async () => {
    try {
      const { setProfileStatus } = await import("@/lib/tauri");
      const ok = await setProfileStatus(status);
      setResult(ok ? "Статус установлен" : "Ошибка");
    } catch (e) {
      setResult(`Ошибка: ${e}`);
    }
  };

  const handleSetAvailability = async () => {
    try {
      const { setProfileAvailability } = await import("@/lib/tauri");
      const ok = await setProfileAvailability(availability);
      setResult(ok ? "Доступность изменена" : "Ошибка");
    } catch (e) {
      setResult(`Ошибка: ${e}`);
    }
  };

  const handleSetIcon = async () => {
    try {
      const id = parseInt(iconId);
      if (isNaN(id)) return;
      const { setProfileIcon } = await import("@/lib/tauri");
      const ok = await setProfileIcon(id);
      setResult(ok ? "Иконка установлена" : "Ошибка");
    } catch (e) {
      setResult(`Ошибка: ${e}`);
    }
  };

  const handleSetBackground = async () => {
    try {
      const id = parseInt(skinId);
      if (isNaN(id)) return;
      const { setProfileBackground } = await import("@/lib/tauri");
      const ok = await setProfileBackground(id);
      setResult(ok ? "Фон установлен" : "Ошибка");
    } catch (e) {
      setResult(`Ошибка: ${e}`);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Кастомизация</h1>
        {result && (
          <Badge variant="secondary">{result}</Badge>
        )}
      </div>

      <div className="grid gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Статус профиля</CardTitle>
          </CardHeader>
          <CardContent className="flex gap-3">
            <Input
              placeholder="Введите статус..."
              value={status}
              onChange={(e) => setStatus(e.target.value)}
              className="flex-1"
            />
            <Button onClick={handleSetStatus}>Установить</Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Доступность</CardTitle>
          </CardHeader>
          <CardContent className="flex gap-3">
            <Select value={availability} onValueChange={(v) => v && setAvailability(v)}>
              <SelectTrigger className="w-[200px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="chat">В сети</SelectItem>
                <SelectItem value="away">Отошёл</SelectItem>
                <SelectItem value="dnd">Не беспокоить</SelectItem>
                <SelectItem value="offline">Невидимка</SelectItem>
                <SelectItem value="mobile">Мобильный</SelectItem>
              </SelectContent>
            </Select>
            <Button onClick={handleSetAvailability}>Применить</Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Иконка профиля</CardTitle>
          </CardHeader>
          <CardContent className="flex gap-3">
            <Input
              placeholder="ID иконки (число)"
              value={iconId}
              onChange={(e) => setIconId(e.target.value)}
              type="number"
              className="w-[200px]"
            />
            <Button onClick={handleSetIcon}>Установить</Button>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Фон профиля</CardTitle>
          </CardHeader>
          <CardContent className="flex gap-3">
            <Input
              placeholder="ID скина для фона"
              value={skinId}
              onChange={(e) => setSkinId(e.target.value)}
              type="number"
              className="w-[200px]"
            />
            <Button onClick={handleSetBackground}>Установить</Button>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
