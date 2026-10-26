# Repair Tools checkpoint

Work in progress: Riot connection diagnostics and repair wizard, external backups and logs, exact strategy selection, GoodLuck account deletion tombstones and identity deduplication.

Latest unshipped changes replace PowerShell Start-Process -Wait with waiting only for the elevated helper, add timeout cancellation markers and optional service.bat live-strategy display backed up outside Zapret.

Pending: immediate wizard status refresh, clearer operation/UAC feedback, live elevated restart and rollback validation. The helper regression test currently fails during temporary-directory cleanup with Windows sharing violation (code 32); the child was still alive as intended. Fix cleanup and rerun before release. Previously built preview4 does not contain these latest changes.

Do not treat network probe success as proof of gameplay, UDP, voice, or every provider. Account sharing from upstream must be preserved when merging.
