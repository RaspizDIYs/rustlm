import type { Metadata } from "next";
import "./globals.css";
import { Sidebar } from "@/components/sidebar";
import { Titlebar } from "@/components/titlebar";
import { LolManagerCleanupDialog } from "@/components/lolmanager-cleanup-dialog";
import { TrayEventHandler } from "@/components/tray-event-handler";
import { UpdateCheckDialog } from "@/components/update-check-dialog";
import { WindowInit } from "@/components/window-init";
import { AutoAcceptProvider } from "@/components/auto-accept-provider";
import { Providers } from "@/app/providers";
import { Geist } from "next/font/google";
import { cn } from "@/lib/utils";
import { Toaster } from "sonner";

const geist = Geist({subsets:['latin','cyrillic'],variable:'--font-sans'});

export const metadata: Metadata = {
  title: "RustLM - LoL Manager",
  description: "League of Legends Account Manager",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="ru" className={cn("dark", "font-sans", geist.variable)}>
      <body className="h-screen bg-transparent">
        <Providers>
        <AutoAcceptProvider>
          <div className="window-frame flex flex-col h-full rounded-lg ring-1 ring-inset ring-border/30 bg-background overflow-hidden shadow-2xl">
            <Titlebar />
            <div className="flex min-h-0 flex-1">
              <Sidebar />
              <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-6">{children}</main>
            </div>
          </div>
          <LolManagerCleanupDialog />
          <TrayEventHandler />
          <UpdateCheckDialog />
          <WindowInit />
          <Toaster position="bottom-right" richColors />
        </AutoAcceptProvider>
        </Providers>
      </body>
    </html>
  );
}
