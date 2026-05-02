"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Lock, FileDown, FileUp, Plus, Trash2, Play, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { WithTooltip } from "@/components/ui/with-tooltip";

import type { LolConfigPreset, LolConfigStatus } from "@/lib/tauri";

type ApplyTarget = { id: string; name: string };

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString("ru-RU", {
      day: "2-digit",
      month: "2-digit",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

export default function ConfigsPage() {
  const [status, setStatus] = useState<LolConfigStatus | null>(null);
  const [presets, setPresets] = useState<LolConfigPreset[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);

  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState("");

  const [applyConfirm, setApplyConfirm] = useState<ApplyTarget | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<ApplyTarget | null>(null);

  const refresh = useCallback(async () => {
    try {
      const tauri = await import("@/lib/tauri");
      const [s, p] = await Promise.all([
        tauri.lolCfgGetStatus(),
        tauri.lolCfgListPresets(),
      ]);
      setStatus(s);
      setPresets(p);
    } catch (e) {
      toast.error(`Не удалось загрузить состояние: ${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleToggleReadonly = async (next: boolean) => {
    if (!status) return;
    setBusy("readonly");
    try {
      const { lolCfgSetReadonly } = await import("@/lib/tauri");
      await lolCfgSetReadonly(next);
      toast.success(next ? "Файл защищён от перезаписи" : "Защита снята");
      await refresh();
    } catch (e) {
      toast.error(`${e}`);
    } finally {
      setBusy(null);
    }
  };

  const handleCreate = async () => {
    const trimmed = createName.trim();
    if (!trimmed) return;
    setBusy("create");
    try {
      const { lolCfgCreatePreset } = await import("@/lib/tauri");
      await lolCfgCreatePreset(trimmed);
      toast.success(`Пресет «${trimmed}» создан`);
      setCreateOpen(false);
      setCreateName("");
      await refresh();
    } catch (e) {
      toast.error(`${e}`);
    } finally {
      setBusy(null);
    }
  };

  const performApply = async (id: string, name: string) => {
    setBusy(`apply-${id}`);
    try {
      const { lolCfgApplyPreset } = await import("@/lib/tauri");
      await lolCfgApplyPreset(id);
      toast.success(`Применён пресет «${name}»`);
      await refresh();
    } catch (e) {
      toast.error(`${e}`);
    } finally {
      setBusy(null);
    }
  };

  const handleApplyClick = (preset: LolConfigPreset) => {
    if (status?.league_running) {
      setApplyConfirm({ id: preset.id, name: preset.name });
      return;
    }
    void performApply(preset.id, preset.name);
  };

  const performDelete = async (id: string, name: string) => {
    setBusy(`delete-${id}`);
    try {
      const { lolCfgDeletePreset } = await import("@/lib/tauri");
      await lolCfgDeletePreset(id);
      toast.success(`Пресет «${name}» удалён`);
      await refresh();
    } catch (e) {
      toast.error(`${e}`);
    } finally {
      setBusy(null);
    }
  };

  const handleExport = async (preset: LolConfigPreset) => {
    setBusy(`export-${preset.id}`);
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const safeName = preset.name.replace(/[\\/:*?"<>|]+/g, "_");
      const path = await save({
        filters: [{ name: "RustLM LoL Config", extensions: ["lolcfg"] }],
        defaultPath: `${safeName}.lolcfg`,
      });
      if (!path) return;
      const { lolCfgExportPreset } = await import("@/lib/tauri");
      await lolCfgExportPreset(preset.id, path);
      toast.success(`Экспортировано в ${path}`);
    } catch (e) {
      toast.error(`${e}`);
    } finally {
      setBusy(null);
    }
  };

  const handleImport = async () => {
    setBusy("import");
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        filters: [{ name: "RustLM LoL Config", extensions: ["lolcfg"] }],
        multiple: false,
      });
      if (!picked) return;
      const path = picked as string;
      const { lolCfgImportPreset } = await import("@/lib/tauri");
      const meta = await lolCfgImportPreset(path);
      toast.success(`Импортирован пресет «${meta.name}»`);
      await refresh();
    } catch (e) {
      toast.error(`${e}`);
    } finally {
      setBusy(null);
    }
  };

  const installNotFound = !loading && status && status.path === null;
  const fileMissing = !loading && status && status.path && !status.exists;
  const canCreate = status?.exists ?? false;
  const canToggle = (status?.exists ?? false) && !busy;

  return (
    <div className="flex flex-col gap-4 p-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Lock className="h-4 w-4" />
            Файл настроек игры
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {loading ? (
            <div className="text-xs text-muted-foreground">Загрузка…</div>
          ) : installNotFound ? (
            <p className="text-sm text-amber-500">
              Не удалось найти установку League of Legends. Запустите Riot Client
              хотя бы один раз и нажмите «Обновить».
            </p>
          ) : (
            <>
              <div className="flex flex-col gap-1">
                <span className="text-[11px] uppercase tracking-wide text-muted-foreground">
                  Путь
                </span>
                <code className="text-xs break-all rounded bg-muted px-2 py-1">
                  {status?.path}
                </code>
                {fileMissing && (
                  <p className="text-xs text-amber-500">
                    PersistedSettings.json ещё не создан — запустите игру хотя бы один раз.
                  </p>
                )}
              </div>

              <div className="flex items-center justify-between gap-3 pt-1">
                <div className="flex flex-col">
                  <span className="text-sm">Защитить от перезаписи (read-only)</span>
                  <span className="text-[11px] text-muted-foreground">
                    Клиент не сможет переписать файл при выходе из игры.
                  </span>
                </div>
                <Switch
                  checked={status?.readonly ?? false}
                  onCheckedChange={(v) => void handleToggleReadonly(Boolean(v))}
                  disabled={!canToggle}
                />
              </div>

              {status?.league_running && (
                <p className="text-xs text-amber-500">
                  League запущен — рекомендуется выйти из клиента перед сменой пресета.
                </p>
              )}
            </>
          )}

          <div className="flex justify-end">
            <Button size="sm" variant="outline" onClick={() => void refresh()}>
              <RefreshCw className="h-3.5 w-3.5" />
              Обновить
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <CardTitle>Пресеты</CardTitle>
            <div className="flex gap-2">
              <WithTooltip
                label={
                  canCreate
                    ? "Сохранить текущие настройки игры как новый пресет"
                    : "Файл настроек ещё не создан"
                }
              >
                <Button
                  size="sm"
                  onClick={() => {
                    setCreateName("");
                    setCreateOpen(true);
                  }}
                  disabled={!canCreate || busy !== null}
                >
                  <Plus className="h-3.5 w-3.5" />
                  Создать из текущих
                </Button>
              </WithTooltip>
              <Button
                size="sm"
                variant="outline"
                onClick={() => void handleImport()}
                disabled={busy !== null}
              >
                <FileUp className="h-3.5 w-3.5" />
                Импорт .lolcfg
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {presets.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              Пресетов пока нет. Создайте первый из текущих настроек игры или
              импортируйте `.lolcfg` файл.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Имя</TableHead>
                  <TableHead className="w-[160px]">Создан</TableHead>
                  <TableHead className="w-[100px]">Версия</TableHead>
                  <TableHead className="w-[200px] text-right">Действия</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {presets.map((p) => (
                  <TableRow key={p.id}>
                    <TableCell className="font-medium">{p.name}</TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatDate(p.created_at)}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      v{p.source_app_version}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end gap-1">
                        <WithTooltip label="Применить">
                          <Button
                            size="icon-sm"
                            variant="ghost"
                            onClick={() => handleApplyClick(p)}
                            disabled={busy !== null || !status?.path}
                          >
                            <Play className="h-3.5 w-3.5" />
                          </Button>
                        </WithTooltip>
                        <WithTooltip label="Экспорт .lolcfg">
                          <Button
                            size="icon-sm"
                            variant="ghost"
                            onClick={() => void handleExport(p)}
                            disabled={busy !== null}
                          >
                            <FileDown className="h-3.5 w-3.5" />
                          </Button>
                        </WithTooltip>
                        <WithTooltip label="Удалить">
                          <Button
                            size="icon-sm"
                            variant="ghost"
                            onClick={() => setDeleteConfirm({ id: p.id, name: p.name })}
                            disabled={busy !== null}
                          >
                            <Trash2 className="h-3.5 w-3.5 text-destructive" />
                          </Button>
                        </WithTooltip>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Новый пресет</DialogTitle>
            <DialogDescription>
              Текущее содержимое PersistedSettings.json будет сохранено под указанным именем.
            </DialogDescription>
          </DialogHeader>
          <Input
            autoFocus
            placeholder="Например: Соревновательный"
            value={createName}
            onChange={(e) => setCreateName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleCreate();
            }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>
              Отмена
            </Button>
            <Button
              onClick={() => void handleCreate()}
              disabled={busy === "create" || !createName.trim()}
            >
              Сохранить
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={applyConfirm !== null}
        onOpenChange={(v) => !v && setApplyConfirm(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Применить пресет?</DialogTitle>
            <DialogDescription>
              Игра запущена. Применённые настройки могут быть перезаписаны клиентом
              при выходе из League. Применить всё равно?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setApplyConfirm(null)}>
              Отмена
            </Button>
            <Button
              onClick={() => {
                if (!applyConfirm) return;
                const t = applyConfirm;
                setApplyConfirm(null);
                void performApply(t.id, t.name);
              }}
            >
              Применить
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={deleteConfirm !== null}
        onOpenChange={(v) => !v && setDeleteConfirm(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Удалить пресет?</DialogTitle>
            <DialogDescription>
              «{deleteConfirm?.name}» будет удалён без возможности восстановления.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteConfirm(null)}>
              Отмена
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (!deleteConfirm) return;
                const t = deleteConfirm;
                setDeleteConfirm(null);
                void performDelete(t.id, t.name);
              }}
            >
              Удалить
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
