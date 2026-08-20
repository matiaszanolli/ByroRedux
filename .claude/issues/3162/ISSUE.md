# SAVE-D4-2026-08-20-01: the new player-facing save/load surface discards CommandOutput into log::info!, so a validation-aborted quicksave is indistinguishable from a written one

**Issue**: #3162 — https://github.com/matiaszanolli/ByroRedux/issues/3162
**Finding ID**: `SAVE-D4-2026-08-20-01`
**Severity**: HIGH
**Dimension**: 4 — Validation Gates
**Audit**: `/audit-save` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: high, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-20.md` (ninth save audit, HEAD `bb0b92f2`)
**Finding ID**: `SAVE-D4-2026-08-20-01`
**Severity**: HIGH
**Dimension**: 4 — Validation Gates (surface owned by Dimension 6)
**Data-Loss Class**: irrecoverable-write — the write that should have happened didn't, and the session has no signal.

## Location

- `byroredux/src/app_events.rs:290-301` — F5 / F9 (`InputAction::Quicksave` / `Quickload`)
- `byroredux/src/main.rs:751-759` — pause-menu Quicksave / Quickload
- `byroredux/src/main.rs:385-393` — the `--load <slot>` boot queue
- against `byroredux/src/save_io.rs:672-686` — the abort branch that *builds* the message
- and `byroredux/src/main.rs:844-848` — the console scrollback the same function already writes to

## Description

#3026 closed by giving save/load a real player surface: `InputAction::Quicksave`/`Quickload`
bound to F5/F9, two pause-menu buttons, and a `--load <slot>` launch flag. All four route
through `save_io::quicksave` / `quickload_latest` / `queue_load_slot`, which correctly share
`SaveCommand`/`LoadCommand`'s implementation — **including the validation gate**. What none of
them share is the gate's **output**.

`SaveCommand::execute` deliberately returns, rather than writes, on a non-empty issue list:

```
"save ABORTED: {n} referential-integrity issue(s) — refusing to write a poisoned save:"
```

followed by up to 20 issues. Every one of the four new call sites collapses that into a log line:

```rust
// byroredux/src/app_events.rs:299-301
if let Some(output) = save_output {
    log::info!("player save action: {}", output.lines.join(" | "));
    return;
}
```

`main.rs:751-759` is the same shape (`log::info!("pause menu quicksave: …")`), and
`main.rs:385-393` the same again for `--load`.

A player with no terminal sees the exact same thing on an abort as on a success: nothing. They
press F5, see and hear no difference, and keep playing on progress that was never written. The
ring cursor correctly does *not* advance (#2017), so the next F5 retries the same slot and
fails the same way — silently, indefinitely.

## Evidence

That this is an omission and not a missing capability is provable from the same function: 90
lines below the pause-menu handler, `apply_debug_ui_outputs` takes `debug_ui: Option<&mut
DebugUiState>` and pushes console-eval responses through `ui.push_console_line(line)`
(`main.rs:846`). **The surface was in scope and unused.**

Reachability of the abort path is not hypothetical, and this cycle made it *more* reachable,
not less:

- `validate_saved_entity_references` (new) aborts on a `Seated.furniture` /
  `FollowState.target_entity` / `EscortState.target_entity` pointing past `next_entity` — e.g.
  an actor seated on furniture that despawned mid-session.
- `validate_equipment` (widened) aborts on an `EquippedWeapon` whose `base_form_id` disagrees
  with `inventory[index]`.
- `validate_progression_state` (new, **#2947**) aborts on **any** `CharacterLevel.xp != 0`.

That last one is the reason to fix this now rather than later: `CharacterLevel`/`Perks` are
save-exempt, so #2947 shipped a runtime tripwire that refuses the whole save the moment XP
accumulates. **The day a leveling runtime lands, F5 stops working for every player at once,
with zero feedback.** Today the abort is a corner case; #2947 makes it universal on a
scheduled, foreseeable date.

## Impact

The subsystem's entire thesis is "refuse to persist a poisoned save rather than seed a
corruption tail." That refusal is correct and now well-covered — but on the only surface a
player can reach, **refusal and success are the same observable event**. The failure mode is
the one the ring design was built to prevent, inverted: instead of F5 eating the old save, F5
silently writes nothing at all.

**Secondary defect in the same block** (distinct from the output-channel gap, same fix commit):
`quickload_latest` picks the newest slot by mtime with **no decode check and no fallback to the
next-newest**, so F9 against a corrupt or stale-`FORMAT_MAJOR` newest slot also fails to
nothing — a dead-ended key with a working save one slot over.

## Related

- **#3026** — CLOSED; the fix that created this surface. This is its *consequence*, not a re-report.
- **#2017** — the ring-cursor half, verified correct.
- **#2947** — CLOSED; `validate_progression_state`, the abort that will make this universal.
- `SAVE-D6-2026-08-20-02` — no test covers any of these four entry points.
- `SAVE-D6-2026-08-20-01` — on the load side, the `log::error!` this leaves is the only trace.

## Suggested Fix

Route the `CommandOutput` of all four call sites into a player-visible channel:

1. At minimum `ui.push_console_line` for the pause-menu pair — the surface already exists in
   the same function (`main.rs:846`).
2. A short on-screen toast / HUD line for F5/F9.
3. `log::warn!` rather than `log::info!` whenever `CommandOutput` is an error or its first line
   starts with `save ABORTED`.
4. Give `quickload_latest` a decode-and-fall-back loop over `list_slots` in descending mtime
   order so a corrupt newest slot doesn't dead-end the key.

## Completeness Checks
- [ ] **SIBLING**: all four entry points fixed, not just F5 — `app_events.rs` (F5/F9), `main.rs` pause menu (×2), `main.rs` `--load`
- [ ] **SIBLING**: the console `save`/`load` command path still produces identical `CommandOutput` after the refactor
- [ ] **TESTS**: a regression test pins that an aborted `quicksave` surfaces a non-log, player-visible line (pairs with `SAVE-D6-2026-08-20-02`)
- [ ] **TESTS**: `quickload_latest` falls back past an undecodable newest slot rather than dead-ending
