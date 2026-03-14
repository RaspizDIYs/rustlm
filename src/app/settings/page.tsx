"use client";

import { useEffect, useState, useCallback } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { RefreshCw, Download } from "lucide-react";

export default function SettingsPage() {
  const [hideLogins, setHideLogins] = useState(false);
  const [autoAcceptMethod, setAutoAcceptMethod] = useState("Polling");
  const [autoUpdate, setAutoUpdate] = useState(true);
  const [minimizeToTray, setMinimizeToTray] = useState(false);
  const [autostart, setAutostart] = useState(false);
  const [autostartBackground, setAutostartBackground] = useState(false);

  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [updateAvailable, setUpdateAvailable] = useState<{ version: string; body?: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  const loadSettings = useCallback(async () => {
    try {
      const { loadSetting, getAutostartEnabled, getAutostartBackground } = await import("@/lib/tauri");
      const [hide, method, update, tray, autostartVal, bgVal] = await Promise.all([
        loadSetting("HideLogins", false),
        loadSetting("AutoAcceptMethod", "Polling"),
        loadSetting("AutoUpdate", true),
        loadSetting("MinimizeToTray", false),
        getAutostartEnabled(),
        getAutostartBackground(),
      ]);
      setHideLogins(hide as boolean);
      setAutoAcceptMethod(method as string);
      setAutoUpdate(update as boolean);
      setMinimizeToTray(tray as boolean);
      setAutostart(autostartVal);
      setAutostartBackground(bgVal);
    } catch {
      // Not in Tauri
    }
  }, []);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  const save = async (key: string, value: unknown) => {
    try {
      const { saveSetting } = await import("@/lib/tauri");
      await saveSetting(key, value);
    } catch {
      // Not in Tauri
    }
  };

  const handleCheckUpdate = async () => {
    setChecking(true);
    setUpdateStatus(null);
    setUpdateAvailable(null);
    try {
      const { checkForUpdate } = await import("@/lib/tauri");
      const result = await checkForUpdate();
      if (result.available && result.version) {
        setUpdateAvailable({ version: result.version, body: result.body });
      } else {
        setUpdateStatus("Вы используете последнюю версию");
      }
    } catch {
      setUpdateStatus("Не удалось проверить обновления");
    } finally {
      setChecking(false);
    }
  };

  const handleInstallUpdate = async () => {
    setInstalling(true);
    try {
      const { installUpdate } = await import("@/lib/tauri");
      await installUpdate();
    } catch {
      setUpdateStatus("Ошибка установки обновления");
      setInstalling(false);
    }
  };

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Настройки</h1>

      <div className="grid gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Интерфейс</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm">Скрывать логины</span>
              <Switch
                checked={hideLogins}
                onCheckedChange={(v) => {
                  setHideLogins(v);
                  save("HideLogins", v);
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm">Сворачивать в трей</span>
              <Switch
                checked={minimizeToTray}
                onCheckedChange={(v) => {
                  setMinimizeToTray(v);
                  save("MinimizeToTray", v);
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm">Запускать вместе с Windows</span>
              <Switch
                checked={autostart}
                onCheckedChange={async (v) => {
                  setAutostart(v);
                  if (!v) setAutostartBackground(false);
                  try {
                    const { setAutostartEnabled } = await import("@/lib/tauri");
                    await setAutostartEnabled(v);
                  } catch {}
                }}
              />
            </div>
            {autostart && (
              <div className="flex items-center justify-between pl-4">
                <span className="text-sm text-muted-foreground">Запускать в фоне</span>
                <Switch
                  checked={autostartBackground}
                  onCheckedChange={async (v) => {
                    setAutostartBackground(v);
                    try {
                      const { setAutostartBackground } = await import("@/lib/tauri");
                      await setAutostartBackground(v);
                    } catch {}
                  }}
                />
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Авто-принятие</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between">
              <span className="text-sm">Метод</span>
              <Select
                value={autoAcceptMethod}
                onValueChange={(v) => {
                  if (v) {
                    setAutoAcceptMethod(v);
                    save("AutoAcceptMethod", v);
                  }
                }}
              >
                <SelectTrigger className="w-[200px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="WebSocket">WebSocket</SelectItem>
                  <SelectItem value="Polling">Polling</SelectItem>
                  <SelectItem value="UIA">UIA</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Обновления</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm">Автоматические обновления</span>
              <Switch
                checked={autoUpdate}
                onCheckedChange={(v) => {
                  setAutoUpdate(v);
                  save("AutoUpdate", v);
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm">
                {updateAvailable
                  ? `Доступна версия ${updateAvailable.version}`
                  : updateStatus ?? "Проверить обновления"}
              </span>
              {updateAvailable ? (
                <Button size="sm" onClick={handleInstallUpdate} disabled={installing}>
                  <Download className="h-3.5 w-3.5 mr-1" />
                  {installing ? "Установка..." : "Установить"}
                </Button>
              ) : (
                <Button variant="outline" size="sm" onClick={handleCheckUpdate} disabled={checking}>
                  <RefreshCw className={`h-3.5 w-3.5 mr-1 ${checking ? "animate-spin" : ""}`} />
                  {checking ? "Проверка..." : "Проверить"}
                </Button>
              )}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
