"use client";

import { useEffect, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Download, Loader2, ChevronDown, ChevronUp } from "lucide-react";
import { CHANGELOG } from "@/lib/changelog";

function ChangelogView() {
  return (
    <div className="max-h-52 overflow-auto text-sm space-y-1 pr-1">
      {CHANGELOG.split("\n").map((line, i) => {
        if (line.startsWith("## ")) {
          return (
            <h4 key={i} className="text-sm font-semibold mt-3 first:mt-0">
              {line.replace("## ", "v")}
            </h4>
          );
        }
        if (line.startsWith("- ")) {
          return (
            <div key={i} className="flex gap-2 text-xs text-muted-foreground ml-2">
              <span className="text-primary shrink-0">•</span>
              <span>{line.replace("- ", "")}</span>
            </div>
          );
        }
        return null;
      })}
    </div>
  );
}

export function UpdateCheckDialog() {
  const [open, setOpen] = useState(false);
  const [version, setVersion] = useState("");
  const [installing, setInstalling] = useState(false);
  const [showChangelog, setShowChangelog] = useState(false);

  useEffect(() => {
    const timer = setTimeout(async () => {
      try {
        const { checkForUpdate } = await import("@/lib/tauri");
        const result = await checkForUpdate();
        if (result.available && result.version) {
          setVersion(result.version);
          setOpen(true);
        }
      } catch {}
    }, 2000);
    return () => clearTimeout(timer);
  }, []);

  const handleInstall = async () => {
    setInstalling(true);
    try {
      const { installUpdate } = await import("@/lib/tauri");
      await installUpdate();
    } catch {
      setInstalling(false);
    }
  };

  if (!open) return null;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Доступно обновление</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <p className="text-sm text-muted-foreground">
            Новая версия <span className="font-medium text-foreground">v{version}</span> готова к установке.
          </p>
          <button
            className="flex items-center gap-1.5 text-sm text-primary hover:underline"
            onClick={() => setShowChangelog((v) => !v)}
          >
            {showChangelog ? <ChevronUp className="h-3.5 w-3.5" /> : <ChevronDown className="h-3.5 w-3.5" />}
            История изменений
          </button>
          {showChangelog && (
            <div className="rounded-md border border-border bg-muted/30 p-3">
              <ChangelogView />
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={installing}>
            Позже
          </Button>
          <Button onClick={handleInstall} disabled={installing}>
            {installing ? (
              <><Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" /> Установка...</>
            ) : (
              <><Download className="h-3.5 w-3.5 mr-1.5" /> Обновить</>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
