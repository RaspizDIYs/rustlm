"use client";

import { useState, useCallback, useEffect } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { ArrowLeft, Bug, Github, MessageCircle, Copy, Check } from "lucide-react";

const DEVELOPERS = [
  { name: "@mejaikin", discord: "mejaikin" },
  { name: "@shp1n4t", discord: "shp1n4t" },
];

const CHANGELOG = `## 0.1.7
- Исправлена автоматизация чемп-селекта: авто-бан и авто-пик теперь работают
- Двухшаговый подход: hover → проверка → lock-in → проверка
- Автоматический повтор при неудаче через WS-события
- Проверка доступности чемпиона перед пиком
- Кастомный тайтлбар с кнопками управления окном
- Системный трей с быстрым доступом к авто-принятию и аккаунтам
- Skeleton-загрузка на странице автоматизации
- Исправлено дублирование логов

## 0.1.6
- Исправлена работа авто-обновлений
- Добавлен тихий режим установки обновлений

## 0.1.3
- Исправлено мелькание консоли при работе приложения
- Предложение удалить старый LolManager при первом запуске

## 0.1.2
- Автоопределение сервера через Riot Client API
- Улучшения интерфейса и стабильности

## 0.1.1
- Шифрованный экспорт/импорт аккаунтов
- Сохранение настроек между сессиями
- Улучшения интерфейса

## 0.1.0
- Начальная версия RustLM
- Полный переход на Tauri v2 + Next.js + shadcn/ui
- Хранилище аккаунтов с DPAPI шифрованием
- Автоматический вход через UIA
- Авто-принятие матча (WebSocket / Polling)
- Авто-пик чемпиона, бана, заклинаний, рун
- Редактор страниц рун
- Кастомизация профиля (статус, иконка, фон, challenges)
- Spy-режим через Riot API
- Совместимость с данными WPF-версии
`;

export default function InfoPage() {
  const [showChangelog, setShowChangelog] = useState(false);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const [appVersion, setAppVersion] = useState("0.1.0");

  useEffect(() => {
    (async () => {
      try {
        const { getVersion } = await import("@tauri-apps/api/app");
        setAppVersion(await getVersion());
      } catch {}
    })();
  }, []);

  const copyDiscord = useCallback((discord: string, index: number) => {
    navigator.clipboard.writeText(discord);
    setCopiedIndex(index);
    setTimeout(() => setCopiedIndex(null), 2000);
  }, []);

  const openUrl = useCallback(async (url: string) => {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(url);
    } catch {
      window.open(url, "_blank");
    }
  }, []);

  if (showChangelog) {
    return (
      <div className="space-y-4">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="sm" onClick={() => setShowChangelog(false)}>
            <ArrowLeft className="h-4 w-4 mr-1" /> Назад
          </Button>
          <h1 className="text-2xl font-bold">История изменений</h1>
        </div>
        <Card>
          <CardContent className="pt-6 prose prose-invert prose-sm max-w-none">
            {CHANGELOG.split("\n").map((line, i) => {
              if (line.startsWith("## ")) {
                return <h3 key={i} className="text-lg font-semibold mt-4 first:mt-0">{line.replace("## ", "")}</h3>;
              }
              if (line.startsWith("- ")) {
                return (
                  <div key={i} className="flex gap-2 text-sm text-muted-foreground ml-2">
                    <span className="text-primary">•</span>
                    <span>{line.replace("- ", "")}</span>
                  </div>
                );
              }
              return null;
            })}
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col items-center justify-center py-8 space-y-3">
        <h1 className="text-4xl font-bold bg-gradient-to-r from-primary to-purple-500 bg-clip-text text-transparent">
          RustLM
        </h1>
        <p className="text-muted-foreground">League of Legends Account Manager</p>
        <p className="text-sm font-medium text-primary">Версия {appVersion}</p>
        <Button variant="link" size="sm" onClick={() => setShowChangelog(true)}>
          Показать историю изменений
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Контакты разработчиков</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {DEVELOPERS.map((dev, i) => (
            <div key={i} className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <MessageCircle className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm font-medium">{dev.name}</span>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => copyDiscord(dev.discord, i)}
              >
                {copiedIndex === i ? (
                  <><Check className="h-3 w-3 mr-1" /> Скопировано</>
                ) : (
                  <><Copy className="h-3 w-3 mr-1" /> Discord</>
                )}
              </Button>
            </div>
          ))}
        </CardContent>
      </Card>

      <div className="flex gap-2 justify-center">
        <Button
          variant="outline"
          size="sm"
          onClick={() => openUrl("https://github.com/RaspizDIYs/rustlm/issues/new")}
        >
          <Bug className="h-4 w-4 mr-1" /> Сообщить о проблеме
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => openUrl("https://github.com/RaspizDIYs/rustlm")}
        >
          <Github className="h-4 w-4 mr-1" /> GitHub
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => openUrl("https://discord.gg/9wS4DBMDNB")}
        >
          <MessageCircle className="h-4 w-4 mr-1" /> Discord
        </Button>
      </div>
    </div>
  );
}
