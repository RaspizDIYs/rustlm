# Repair Tools checkpoint — 2026-10-27

Updated from origin/master bf43ee4 (LM 0.4.2), preserving GoodLuck account sharing, friends and PUUID improvements.

PowerShell helper waits only for its own process; its long-running-child regression test now passes. Latest user diagnostics show three successful Zapret launches without stderr output and one completed wizard run. This does not establish game readiness: the local client check returned not_installed.

Current changes keep status visible while busy, refresh after startup, show elapsed process-control time and actionable timeout messages. A candidate which loses previously working critical IPv4 services is measured again and rolled back if the same loss persists. Per-address CDN changes and unavailable IPv6 are not counted as service regressions.

Validation: 56 Rust tests passed, one live-network smoke ignored; 9 JS tests passed; TypeScript and production frontend build passed. Native UI/UAC and the new rollback decision have not been exercised end to end. No system network settings changed during this investigation.

Local diagnostic summary and seven structured reports: artifacts/repair-tools-audit-20261027 (gitignored). No raw account/client files included. LM 0.4.3 executable built with the updated UI and encrypted account vault. See docs/lm-0.4.3.md.
