"use client";

import { useState, useEffect, useCallback } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { totpSetup, totpConfirmSetup } from "@/lib/tauri";
import type { TotpSetupInfo } from "@/lib/tauri";
import { Copy, Check, ShieldCheck } from "lucide-react";
import { toast } from "sonner";

interface TotpSetupDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onComplete?: () => void;
}

type Step = "qr" | "verify" | "recovery";

export function TotpSetupDialog({
  open,
  onOpenChange,
  onComplete,
}: TotpSetupDialogProps) {
  const [step, setStep] = useState<Step>("qr");
  const [setupInfo, setSetupInfo] = useState<TotpSetupInfo | null>(null);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [recoveryCodes, setRecoveryCodes] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  const initSetup = useCallback(async () => {
    try {
      setLoading(true);
      const info = await totpSetup();
      setSetupInfo(info);
      const QRCode = await import("qrcode");
      const dataUrl = await QRCode.toDataURL(info.otpauthUri, {
        width: 200,
        margin: 2,
        color: { dark: "#ffffff", light: "#00000000" },
      });
      setQrDataUrl(dataUrl);
    } catch (e) {
      toast.error(`Ошибка настройки 2FA: ${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) {
      setStep("qr");
      setCode("");
      setRecoveryCodes([]);
      setCopied(false);
      initSetup();
    }
  }, [open, initSetup]);

  const handleVerify = async () => {
    if (code.length !== 6) return;
    setLoading(true);
    try {
      const codes = await totpConfirmSetup(code);
      setRecoveryCodes(codes);
      setStep("recovery");
      toast.success("2FA успешно включена");
    } catch (e) {
      toast.error(`Неверный код: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const handleCopyRecovery = async () => {
    try {
      await navigator.clipboard.writeText(recoveryCodes.join("\n"));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {}
  };

  const handleDone = () => {
    onOpenChange(false);
    onComplete?.();
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ShieldCheck className="h-5 w-5" />
            Настройка 2FA
          </DialogTitle>
          <DialogDescription>
            {step === "qr" && "Отсканируйте QR-код в Google Authenticator"}
            {step === "verify" && "Введите 6-значный код из приложения"}
            {step === "recovery" && "Сохраните коды восстановления"}
          </DialogDescription>
        </DialogHeader>

        {step === "qr" && (
          <div className="space-y-4">
            <div className="flex justify-center">
              {qrDataUrl ? (
                <img src={qrDataUrl} alt="TOTP QR" className="h-[200px] w-[200px]" />
              ) : (
                <div className="h-[200px] w-[200px] bg-muted rounded animate-pulse" />
              )}
            </div>
            {setupInfo && (
              <div className="space-y-1">
                <p className="text-xs text-muted-foreground">
                  Или введите ключ вручную:
                </p>
                <code className="block text-xs bg-muted p-2 rounded font-mono break-all select-all">
                  {setupInfo.secret}
                </code>
              </div>
            )}
            <Button
              className="w-full"
              onClick={() => setStep("verify")}
              disabled={!setupInfo}
            >
              Далее
            </Button>
          </div>
        )}

        {step === "verify" && (
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
        )}

        {step === "recovery" && (
          <div className="space-y-4">
            <p className="text-sm text-destructive font-medium">
              Сохраните эти коды. Они не будут показаны повторно.
            </p>
            <div className="grid grid-cols-2 gap-2 bg-muted p-3 rounded">
              {recoveryCodes.map((c) => (
                <code key={c} className="text-xs font-mono">
                  {c}
                </code>
              ))}
            </div>
            <Button variant="outline" className="w-full" onClick={handleCopyRecovery}>
              {copied ? (
                <>
                  <Check className="h-4 w-4 mr-2" /> Скопировано
                </>
              ) : (
                <>
                  <Copy className="h-4 w-4 mr-2" /> Скопировать все
                </>
              )}
            </Button>
            <Button className="w-full" onClick={handleDone}>
              Готово
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
