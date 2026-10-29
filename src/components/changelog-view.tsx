import { changelogSections } from "@/lib/changelog";
import { cn } from "@/lib/utils";

export function ChangelogView({
  activeVersion,
  className,
}: {
  activeVersion: string;
  className?: string;
}) {
  return (
    <div className={cn("accounts-page flex flex-col gap-5", className)}>
      {changelogSections().map(({ version, items }) => {
        const active = version === activeVersion.replace(/^v/, "");
        return (
          <section
            key={version}
            aria-label={`Версия ${version}`}
            data-current-version={active || undefined}
            className={cn(
              "flex flex-col gap-2 border-l-2 pl-3",
              active ? "border-primary/50" : "border-transparent",
            )}
          >
            <h3 className="text-sm font-semibold">
              <span
                className={cn(
                  active &&
                    "rounded-sm bg-primary/15 px-1 py-0.5 underline decoration-primary/50 underline-offset-4",
                )}
              >
                {version}
              </span>
            </h3>
            <ul className="flex list-disc flex-col gap-1 pl-4 text-xs leading-6 text-muted-foreground">
              {items.map((item, index) => (
                <li key={index}>
                  {active ? (
                    <mark className="box-decoration-clone rounded-sm bg-primary/15 px-1 py-0.5 text-foreground">
                      {item}
                    </mark>
                  ) : (
                    item
                  )}
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}
