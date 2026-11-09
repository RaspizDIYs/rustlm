# Home report investigation — 2026-11-09

Source: `RustLM-report-1793980173008.json`, RustLM 0.5.25, report timestamp
2026-11-06T15:49:40Z. The supplied report is not committed because it contains local paths.

## Evidence and limits

The report contains no network measurements (`network_reports: []`). Its 127 TIMEOUT
and 10 HTTP_403 entries are keyword matches in log tails, not counts of confirmed
failed requests or blocked Riot addresses. They do not establish a provider/DNS cause.
The reported failures were investigated against the released code; a live League
client, the original provider and elevated Windows service control are unavailable here.

## Changes

- Zapret: inspect and restart the existing service belonging to the selected executable
  through CIM rather than refusing all service mode. Reject ambiguous services, mixed
  standalone/service processes and concurrent configuration changes. Save original
  service arguments/state before mutation and restore them on failure. Keep dry-run
  validation, selected-process scope and elevated-helper receipts. Stop uses service
  control for the selected service. Service creation, deletion and unrelated installs
  are outside this change.
- Party: preserve authorization, connection-in-progress, rate-limit and server/network
  errors instead of always telling the user to open League.
- Riot icon proof: respect retry delays, recognize unsent LCU cooldown errors, recheck
  account identity at write dispatch and allow up to 180 seconds for propagation within
  the existing overall challenge deadline. An expired stage remains resumable; this
  does not prove Riot propagation succeeds on the original machine.
- Guest configs: retain the closed-client safety guard. Show the expected waiting state
  as information and explain that originals are saved; no writing through an active game.
- Reports: schema v2 includes bounded, sanitized repair-session summaries, LCU diagnostics
  and Unglitch state, and scans application logs. Export no raw account credentials or
  arbitrary session error text. Avoid counting timeout configuration values as failures.

The service implementation uses the platform's named `PathName` argument of
[Win32_Service.Change](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/change-method-in-class-win32-service).

## Verification

- Rust library: 290 passed, 34 ignored, zero failures.
- JavaScript regression suite: 104 passed, zero failures.
- Native LCU integration: 32 passed; recorded replay: one passed, zero mismatches.
- Service simulation: seven scenarios, zero actual service commands and zero foreign
  service mutations. The actual generated restart template also passes PowerShell parsing.
  Covers running/stopped services, failed startup and restore, cancellation, concurrent
  change and ambiguous selection. This is not an elevated SCM/UAC test.
- TypeScript check and frontend production build passed; `git diff --check` passed.

## Remaining live checks

Verify elevated service restart/rollback under the user's actual service policy,
Riot icon confirmation, party connection and config restoration with League installed.
Collect a schema-v2 report if these fail. No installer, release, deployment or version
change is included in this investigation.
