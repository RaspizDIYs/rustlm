"use client";

import { useEffect, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Trash2 } from "lucide-react";
import {
  checkLolManagerInstalled,
  uninstallLolManager,
  loadSetting,
  saveSetting,
} from "@/lib/tauri";

export function LolManagerCleanupDialog() {
  const [open, setOpen] = useState(false);
  const [uninstalling, setUninstalling] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const dismissed = await loadSetting<boolean>("LolManagerCleanupDismissed", false);
        if (dismissed) return;
        const installed = await checkLolManagerInstalled();
        if (installed) setOpen(true);
      } catch {
        // ignore
      }
    })();
  }, []);

  const handleUninstall = async () => {
    setUninstalling(true);
    setError(null);
    try {
      await uninstallLolManager();
      setDone(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setUninstalling(false);
    }
  };

  const handleDismiss = async () => {
    await saveSetting("LolManagerCleanupDismissed", true);
    setOpen(false);
  };

  const handleClose = async (isOpen: boolean) => {
    if (!isOpen) {
      await saveSetting("LolManagerCleanupDismissed", true);
      setOpen(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            <div className="flex items-center gap-2">
              <Trash2 className="h-5 w-5" />
              Удалить LolManager?
            </div>
          </DialogTitle>
          <DialogDescription>
            На вашем компьютере обнаружен LolManager. Так как вы уже используете
            RustLM, старая версия больше не нужна. Хотите удалить LolManager?
          </DialogDescription>
        </DialogHeader>

        {error && <p className="text-sm text-destructive">{error}</p>}

        {done ? (
          <div className="text-sm text-muted-foreground">
            LolManager успешно удалён.
          </div>
        ) : null}

        <DialogFooter>
          {done ? (
            <Button onClick={() => setOpen(false)}>Закрыть</Button>
          ) : (
            <>
              <Button variant="outline" onClick={handleDismiss}>
                Не сейчас
              </Button>
              <Button
                variant="destructive"
                onClick={handleUninstall}
                disabled={uninstalling}
              >
                {uninstalling ? "Удаление..." : "Удалить LolManager"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
