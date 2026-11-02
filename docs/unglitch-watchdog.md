# Autonomous Unglitch

Implement a background monitor, shared manual/automatic operation lock, native
window responsiveness and process checks, compact opt-out switch and status.
Use repeated evidence, startup grace, a single soft reload per incident and a
cooldown. Never auto-hide, force-kill, restart the game, or reload in an unknown,
matchmaking, ready-check, champion-select or active-game phase.

Validation: policy tests for protected phases, startup, deliberate hiding,
transient errors, cooldown and disabled monitoring; synthetic LCU recovery and
failed recovery; native/frontend builds and local launch. Synthetic checks do
not validate live League UI freezes or real RU/EUW match behavior.

The monitor is on by default; the Unglitch header switch persists the local
`UnglitchAutoRecovery` preference. Checks run every five seconds while LM is
open, including in the tray. A new backend receives 90 seconds to start;
suspected hangs must persist for 30 seconds. A sampling gap over 15 seconds
starts the evidence window again (for example after PC sleep).

Automatic recovery is limited to `None`, `Lobby`, and `EndOfGame`, with a
verified installation, current LCU session, no running game process, and an
answering API. Slow responses or an unchanged phase alone do not trigger it.
Unknown window responsiveness also does not count as a hang. A deliberately
hidden UI is left alone. Manual actions pause monitoring decisions for two
minutes. Only one soft reload is attempted per incident; a later incident needs
60 seconds of observed health and at least ten minutes since the last attempt.

Manual and automatic actions share the same operation lock. A reload is
confirmed only when the UI process changed and three successive checks find
an answering window, renderer and API. This still cannot prove that every
League screen or animation is correct. No screen capture/OCR, injection, forced
automatic termination or speculative action during a match is used.

State transitions and recovery results go into LM's existing logs. Synthetic
LCU tests emulate both successful and unsuccessful UI recovery; they do not
terminate real processes.
