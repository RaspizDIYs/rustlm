# LM 0.4.3: devices, friends and repair tools

## Encrypted account sync

Sign into the same GoodLuck profile on both PCs. On the first PC, open Settings → My PCs and create a vault. Save the generated recovery code in a password manager. On the second PC enter that code; the first PC may be offline. The code is a reusable vault key, not an OTP sent to another device.

AES-256-GCM encrypts the account document on the client with user/protocol associated data and a fresh random nonce. The key, baseline and pending rotation key are stored under Windows DPAPI separately from synchronised settings. There is no plaintext upload fallback. Existing cloud-only records migrate; local records take precedence on initial migration. Previous server backups may still contain legacy plaintext data: client migration cannot erase those backups.

Synchronization performs a three-way merge against the last encrypted local baseline. Deletions propagate; conflicting edits require choosing this PC or cloud in Settings. The server uses an atomic compare-and-update revision; old clients cannot overwrite an encrypted vault. A missing/deleted vault never silently clears local accounts. Corrupt local storage cannot be uploaded as an empty document. Synchronisation runs after changes and every 30 seconds while connected.

A recovery code grants full access to the vault. Changing the key disconnects other PCs from future synchronization; their existing local copies remain. Reconnect them using the new saved code. This is not remote erasure or a hardware security boundary against a compromised PC. Account sharing with friends uses its separate existing transport and is not covered by the vault encryption claim.

## Friends

Accounts → Friends shows Riot friends in LM and GoodLuck friends as separate categories. Discovery is opt-in; only verified linked Riot identities and reciprocal published rosters match. The current logged-in League client supplies its friend list. Sign into each account to accumulate the combined list; cached rosters expire after 30 days. GoodLuck friendship is optional for Riot friends; existing GoodLuck sharing still works. Blocking suppresses discovery and new shares.

Riot identity verification in the GoodLuck profile is required to prevent someone claiming another player's PUUID. Local roster data is supplied by the desktop client, not cryptographic proof from Riot. Clients holding verified identities remain trusted to report their own rosters honestly. LCU availability and shape can change.

## Auto accept

WebSocket accepts on ready-check events with a slow polling fallback. Polling uses a fast ready-check loop. UIA invokes an exact named enabled acceptance button only inside LeagueClientUx; it requires an accessible client window and does not blindly click screen coordinates. Changing the Settings method updates the running service and persisted automation settings. Champion automation remains active independently.

## Validation limits

Tests cover authenticated encryption rejection, merge conflicts/deletions, safe migration, stale/corrupt storage, UIA name restrictions, verified mutual rosters, legacy downgrade prevention and PostgreSQL concurrent writes using real migrations. Real play, two physical PCs and live UIA acceptance still require user-side checks. No account vault was created or migrated for the user during development.
