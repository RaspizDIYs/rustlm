# External client diagnostics (no Pengu)

Scope: measure LM's own LCU requests, distinguish HTTP acknowledgement from
the existing pick/ban verification, and inspect accessible native UI elements.
No Pengu plugin, CEF hooks, injected DLL, client DOM access or automatic UI clicks.

The Unglitch diagnostics card reports per-area last/max latency, request/error
counts, and the last 24 noteworthy events (six displayed). Normal polling and
expected GET 404 responses do not flood the event list. A slow response is not
proof of a frozen screen. Reports contain categories/numbers/status codes, not
URLs, query strings, request/response bodies, passwords or account identifiers.

Windows UIA checks run on a worker every 15 seconds while LM is open, independently
of the automatic recovery switch. Only one worker may inspect the client at a
time. COM transaction limits, a 120-element/depth-8/time budget and a three-second
outer timeout bound the probe; a timed-out worker keeps its permit until it
actually returns. Unknown accessibility remains unknown. Only selected known
tab names are transiently classified into Loot/Stats; raw names are not stored.
Progress bars are counted only if exposed by UIA and not offscreen. A continuous
indicator is shown as waiting, never as conclusive evidence of a broken screen.
Sampling gaps reset the waiting duration.

The normal Unglitch recovery policy does not consume these metrics. This stage
does not reload a screen or repeat crafting, chest-opening, picks or purchases.
It cannot see every request originating inside League or all CSS animations.
The synthetic LCU explicitly reports that no real UI was inspected.

Validation: bounded/privacy tests, expected-404 handling, acknowledgement versus
verification, selected-tab classification, synthetic LCU transport integration,
frontend/native builds. Real League UIA coverage, real animation freezes and
provider behavior must be checked at home; see home-validation-checklist.md.
