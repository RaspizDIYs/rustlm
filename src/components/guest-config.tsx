"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import type { GuestConfigMode, GuestConfigStatus, LolConfigPreset } from "@/lib/tauri";

export function GuestConfig({ presets, onChange }: { presets: LolConfigPreset[]; onChange: () => Promise<void> }) {
  const [status, setStatus] = useState<GuestConfigStatus | null>(null);
  const [error, setError] = useState("");
  const [id, setId] = useState<string | null>(null);
  const [mode, setMode] = useState<GuestConfigMode>("keyboard");
  const [busy, setBusy] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [saveName, setSaveName] = useState("");
  const busyRef = useRef(false);
  const mountedRef = useRef(false);
  const statusEpoch = useRef(0);
  const refresh = useCallback(async () => {
    if (!mountedRef.current) return;
    const epoch = ++statusEpoch.current;
    try {
      const api = await import("@/lib/tauri");
      const next = await api.lolCfgGuestStatus();
      if (!mountedRef.current || epoch !== statusEpoch.current) return;
      setStatus(next);
      setError("");
    } catch (e) {
      if (mountedRef.current && epoch === statusEpoch.current) setError(String(e));
    }
  }, []);
  useEffect(() => {
    mountedRef.current = true;
    void refresh();
    const timer = setInterval(() => { if (!busyRef.current) void refresh(); }, 5000);
    return () => {
      mountedRef.current = false;
      ++statusEpoch.current;
      clearInterval(timer);
    };
  }, [refresh]);
  const run = async (restore: boolean) => {
    if (busyRef.current || !mountedRef.current || (!restore && !id)) return;
    busyRef.current = true;
    ++statusEpoch.current;
    setBusy(true);
    try {
      const api = await import("@/lib/tauri");
      if (restore) await api.lolCfgGuestRestore(true, saveName.trim() || null);
      else if (id) await api.lolCfgGuestStart(id, mode);
      if (mountedRef.current) {
        setRestoreOpen(false);
        setSaveName("");
        toast.success(restore ? "Исходные настройки восстановлены" : mode === "keyboard" ? "Клавиши применены временно" : "Весь пресет применён временно");
      }
    } catch (e) { if (mountedRef.current) toast.error(String(e)); }
    finally {
      try {
        if (mountedRef.current) {
          await refresh();
          if (mountedRef.current) await onChange();
        }
      } catch (e) { if (mountedRef.current) toast.error(String(e)); }
      finally {
        busyRef.current = false;
        if (mountedRef.current) setBusy(false);
      }
    }
  };
  const selectPersonalPreset = async (next: string | null) => {
    if (!next || busyRef.current || !mountedRef.current) return;
    busyRef.current = true;
    ++statusEpoch.current;
    setBusy(true);
    try {
      const api = await import("@/lib/tauri");
      await api.lolCfgGuestSetDefault(next);
    } catch (e) { if (mountedRef.current) toast.error(String(e)); }
    finally {
      try { await refresh(); }
      finally { busyRef.current = false; if (mountedRef.current) setBusy(false); }
    }
  };
  return <section aria-label="Гостевой режим" className="space-y-2 px-3">
    <div className="flex flex-wrap items-center justify-between gap-2"><div><h2 className="text-sm font-semibold">Гостевой режим</h2><p className="text-xs text-muted-foreground">На общих аккаунтах ваши клавиши применяются автоматически.</p></div><Button size="sm" variant="ghost" disabled={busy} onClick={() => { if (!busyRef.current) void refresh(); }}>Проверить восстановление</Button></div>
      <div className="flex flex-wrap items-center justify-between gap-3 py-1">
        <span className="text-xs font-medium">Автоматически на общих аккаунтах</span>
        <Select disabled={busy || !!status?.active} value={status?.personal_preset_id ?? null} onValueChange={value => void selectPersonalPreset(value)}>
          <SelectTrigger className="w-56" aria-label="Личный пресет для общих аккаунтов"><SelectValue placeholder="Сохранятся при первом входе">{presets.find(p => p.id === status?.personal_preset_id)?.name}</SelectValue></SelectTrigger>
          <SelectContent><SelectGroup>{presets.map(p => <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>)}</SelectGroup></SelectContent>
        </Select>
      </div>
      {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
      {status?.active ? <>
        <p role="status" className="text-xs text-amber-500">{status.changed ? "Настройки изменились. Сохранить или восстановить?" : "Гостевой режим активен. Исходные настройки сохранены."}</p>
        <div className="flex justify-end"><Button size="sm" disabled={busy || !!error} onClick={() => { if (!busyRef.current) setRestoreOpen(true); }}>Восстановить…</Button></div>
      </> : <div className="flex flex-wrap items-end justify-between gap-3 py-1">
        <span className="self-center text-xs font-medium">Применить вручную</span>
        <div className="flex flex-wrap items-end justify-end gap-2">
        <div className="flex items-center gap-2"><span className="text-[11px] text-muted-foreground">Применить</span>
        <Select disabled={busy} value={mode} onValueChange={value => { if (value === "keyboard" || value === "full") setMode(value); }}>
          <SelectTrigger className="w-36" aria-label="Какие настройки применить"><SelectValue>{mode === "keyboard" ? "Клавиши" : "Весь пресет"}</SelectValue></SelectTrigger>
          <SelectContent><SelectGroup><SelectItem value="keyboard">Клавиши</SelectItem><SelectItem value="full">Весь пресет</SelectItem></SelectGroup></SelectContent>
        </Select>
        </div>
        <div className="flex items-center gap-2"><span className="text-[11px] text-muted-foreground">Из</span>
        <Select disabled={busy} value={id} onValueChange={setId}>
          <SelectTrigger className="w-56" aria-label="Гостевой пресет"><SelectValue placeholder="Выберите пресет">{presets.find(p => p.id === id)?.name}</SelectValue></SelectTrigger>
          <SelectContent><SelectGroup>{presets.map(p => <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>)}</SelectGroup></SelectContent>
        </Select>
        </div>
        <Button size="sm" disabled={busy || !status || !!error || !id || !presets.some(p => p.id === id)} onClick={() => void run(false)}>Применить временно</Button>
        </div>
      </div>}
      <p role="status" className="text-xs text-muted-foreground">{mode === "keyboard" ? "Клавиши: заменяются только назначения клавиш, графика этого ПК сохраняется." : "Весь пресет: заменяются все сохранённые настройки, включая графику; перед применением закройте League."}</p>
      <Dialog open={restoreOpen} onOpenChange={open => { if (!busyRef.current) setRestoreOpen(open); }}>
        <DialogContent>
          <DialogHeader><DialogTitle>Восстановить исходные настройки?</DialogTitle><DialogDescription>Текущие настройки будут заменены исходными. Укажите имя, чтобы сначала сохранить изменения в пресет.</DialogDescription></DialogHeader>
          <Input aria-label="Имя личного пресета перед восстановлением" placeholder="Имя пресета (необязательно)" value={saveName} onChange={e => setSaveName(e.target.value)} disabled={busy} />
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => { if (!busyRef.current) setRestoreOpen(false); }}>Отмена</Button>
            <Button disabled={busy} onClick={() => void run(true)}>{saveName.trim() ? "Сохранить и восстановить" : "Заменить исходными"}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
  </section>;
}
