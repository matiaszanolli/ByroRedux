# #3772 — UI-D2-2026-08-30-02: the #2969 drop-counter latch is not reset on menu swap, so a new menu's first N drops are silent when the previous menu dropped N

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, ui, bug

---

**Audit**: `/audit-ui` — `docs/audits/AUDIT_UI_2026-08-30.md` (Dimension 2 — Host Bridge Transport), HEAD `64f64480`
**Finding ID**: `UI-D2-2026-08-30-02`

- **Severity**: LOW
- **Status**: NEW
- **Profile**: both

## Location

- `byroredux/src/app_frame.rs:277-291` — the latch read + write
- `byroredux/src/main.rs:141-143` — `host_call_gap`
- `byroredux/src/main.rs:246` / `:571` — `App::ui_dropped_host_calls` declaration and its only initialisation

## Description

`app_frame.rs` latches the reading in `self.ui_dropped_host_calls`, which lives on `App` and is **never reset**.

`host_call_gap` (`reported.checked_sub(latched).filter(|l| *l > 0)`) deliberately treats a *decrease* as a menu swap and reports nothing — correct, that is what stops `checked_sub` wrapping. But the latch is then overwritten with the new, smaller value, and no code path zeroes it at `install_player` time.

The correct case:

| frame | menu | `dropped_calls()` | latch before | warned? |
|---|---|---|---|---|
| n | A | 1000 | 900 | yes, "lost 100" |
| n+1 | A | 1000 | 1000 | no |
| n+2 | B (fresh bridge) | 0 | 1000 | no — reset, latch ← 0 |

The lossy case is a swap landing on a frame where the new bridge has *already* evicted:

| frame | menu | `dropped_calls()` | latch before | warned? |
|---|---|---|---|---|
| n | A | 1000 | 1000 | no |
| n+1 | B (fresh bridge, dropped 999 during load/first tick) | 999 | 1000 | **no** — 999 < 1000 reads as a reset |
| n+2 | B | 1000 | 999 | yes, "lost 1" |

Menu B's first 999 lost calls are reported as one. The latch cannot distinguish "same bridge, impossible decrease" from "different bridge, smaller count", because it latches a **number** and not the **identity** of the bridge that produced it.

## Evidence

Re-verified at HEAD: `grep -n 'ui_dropped_host_calls' byroredux/src/*.rs` returns exactly four sites — `app_frame.rs:278` (read), `app_frame.rs:290` (write), `main.rs:246` (field), `main.rs:571` (init to 0). No reset site.

## Impact

Diagnostic-only today (nothing routes host calls into game state yet), and it needs a menu that overflows the 1024-entry bound at all, which no measured vanilla menu does — `docs/engine/ui.md:601-610` records at most one host call across 600 ticked frames for every corpus menu tested. Hence LOW.

Filed because this is the exact channel #2969 exists to provide, the fix is three lines against data already in scope, and the failure is silent: it costs nothing observable until the drain starts routing calls into quest / inventory / player state, at which point it is a lost state transition with no signal.

## Related

- #2969 (CLOSED — the drop-counter channel this is the residual of)

## Suggested Fix

`app_frame.rs` already holds `ui.menu_name` in the same block and already keys `ui_reported_host_methods` by it. Latch the pair `(menu_name, dropped)` — or zero `self.ui_dropped_host_calls` whenever `ui.menu_name` differs from the name seen last frame — so the swap case becomes an **explicit** reset rather than an inferred one. `host_call_gap`'s decrease guard then stays as the belt-and-braces it was meant to be.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `ui_reported_host_methods` and any other `App`-resident latch over per-menu state
- [ ] **TESTS**: A regression test pins this specific fix — a menu swap to a bridge that has already dropped N < latched must report all N, not zero
