import type { Metadata } from "next";
import "./globals.css";
import { Sidebar } from "@/components/sidebar";
import { Titlebar } from "@/components/titlebar";
import { LolManagerCleanupDialog } from "@/components/lolmanager-cleanup-dialog";
import { TrayEventHandler } from "@/components/tray-event-handler";
import { Geist } from "next/font/google";
import { cn } from "@/lib/utils";

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
        <div className="window-frame flex flex-col h-full rounded-lg border border-border bg-background overflow-hidden shadow-2xl">
          <Titlebar />
          <div className="flex flex-1 min-h-0">
            <Sidebar />
            <main className="flex-1 overflow-auto p-6">{children}</main>
          </div>
        </div>
        <LolManagerCleanupDialog />
        <TrayEventHandler />
      </body>
    </html>
  );
}
