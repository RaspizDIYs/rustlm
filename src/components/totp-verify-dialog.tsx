"use client";

import { useState, useEffect } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ShieldCheck } from "lucide-react";
import { totpValidate } from "@/lib/tauri";
import { toast } from "sonner";

interface TotpVerifyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onVerified?: () => void;
  cloudHint?: boolean;
}

export function TotpVerifyDialog({
  open,
  onOpenChange,
  onVerified,
  cloudHint,
}: TotpVerifyDialogProps) {
  const [code, setCode] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (open) {
      setCode("");
    }
  }, [open]);

  const handleVerify = async () => {
    if (code.length !== 6) return;
    setLoading(true);
    try {
      await totpValidate(code);
    } catch (e) {
      toast.error(`Неверный код: ${e}`);
      return;
    } finally {
      setLoading(false);
    }
    toast.success("2FA подтверждена");
    setCode("");
    try {
      await onVerified?.();
    } catch (e) {
      toast.error(String(e));
    }
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[360px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ShieldCheck className="h-5 w-5" />
            Двухфакторная аутентификация
          </DialogTitle>
          <DialogDescription>
            {cloudHint
              ? "Код из Google Authenticator для облака (сессия ~1 ч). После истечения введи новый код — 2FA отключать не нужно."
              : "Введите код из Google Authenticator"}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <Input
            placeholder="000000"
            value={code}
            onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
            className="text-center text-lg tracking-[0.5em] font-mono"
            autoFocus
            onKeyDown={(e) => e.key === "Enter" && handleVerify()}
          />
          <Button
            className="w-full"
            onClick={handleVerify}
            disabled={code.length !== 6 || loading}
          >
            {loading ? "Проверка..." : "Подтвердить"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
