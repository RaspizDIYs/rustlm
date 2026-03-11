"use client";

import { useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";

const navItems = [
  { href: "/accounts", label: "Аккаунты", icon: "👤" },
  { href: "/automation", label: "Автоматизация", icon: "⚡" },
  { href: "/customization", label: "Кастомизация", icon: "🎨" },
  { href: "/spy", label: "Разведка", icon: "🔍" },
  { href: "/info", label: "Информация", icon: "ℹ️" },
  { href: "/settings", label: "Настройки", icon: "⚙️" },
  { href: "/logs", label: "Логи", icon: "📋" },
];

export function Sidebar() {
  const [collapsed, setCollapsed] = useState(false);
  const pathname = usePathname();

  return (
    <aside
      className={cn(
        "flex flex-col h-full bg-sidebar border-r border-border transition-all duration-300",
        collapsed ? "w-[60px]" : "w-[220px]"
      )}
    >
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b border-border">
        {!collapsed && (
          <span className="text-sm font-bold text-foreground tracking-wide">
            RustLM
          </span>
        )}
        <button
          onClick={() => setCollapsed(!collapsed)}
          className="p-1.5 rounded-md hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
        >
          {collapsed ? "▶" : "◀"}
        </button>
      </div>

      {/* Navigation */}
      <nav className="flex-1 flex flex-col gap-1 p-2">
        {navItems.map((item) => {
          const isActive =
            pathname === item.href || pathname.startsWith(item.href + "/");
          return (
            <Link
              key={item.href}
              href={item.href}
              className={cn(
                "flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-all duration-200",
                isActive
                  ? "bg-gradient-to-r from-primary to-purple-500 text-primary-foreground shadow-lg shadow-primary/25"
                  : "text-sidebar-foreground hover:bg-accent hover:text-foreground"
              )}
            >
              <span className="text-base flex-shrink-0">{item.icon}</span>
              {!collapsed && <span>{item.label}</span>}
            </Link>
          );
        })}
      </nav>

      {/* Footer */}
      <div className="p-3 border-t border-border">
        {!collapsed && (
          <div className="text-xs text-muted-foreground">v0.1.0</div>
        )}
      </div>
    </aside>
  );
}
