**HEAD**: 66941d0c0 (audited) · **Baseline**: AUDIT_SCRIPTING_2026-10-31.md (670f81917) · **Scope**: 107 scripting-runtime commits since 10-31.

# Scripting Runtime Audit: 2026-11-15

**Test runs**: `byroredux-scripting` passed 1411 tests (11 ignored), and was clean under `BYRO_LOCK_ORDER_CHECK=1`.

| ID | Sev | Finding | Outcome |
|---|---|---|---|
| SCR-2026-11-15-01 | HIGH | The ObScript VM had no `%`. The unknown-operator arm pushed 0 without consuming its operands. In FalloutNV, `VNovacChupacabraNightkinSCRIPT` read `timerBark % 31 == 0` as always true, so the bark block fired every frame. The CS *Set* page documents `%` as integer modulus. | b485627e3: `%` implemented. The unknown arm can only be reached by corrupt bytecode and now logs once. Test `a_percent_is_integer_modulus`. |
| SCR-2026-11-15-02 | MEDIUM | `PendingScriptedLines` now carries INFO result scripts, but it was neither saved nor discarded. On a live load, a line from the abandoned timeline ran its End script in the loaded game. A save taken mid-line loses that End script. | cfff182ea: a live load discards the lines. The save direction needs a format change: #6944. |
| SCR-2026-11-15-03 | LOW | `IsLimbGone` answered "not gone" for an actor it could not resolve. | fcc138ed9: it now declines, as `GetKillingBlowLimb` does. |

**Noted, not filed (needs a decision)**: a VM-bridged getter that declines reads as `0.0` inside a condition. `run_getter_for_condition(..).unwrap_or(0.0)` does this, and `is_modeled` is true for the whole catalog row, so an `== 0` gate passes on a decline. Two examples are `GetDetected` with no `SightQuery` and `IsInInterior` with no cell kind. No vanilla CTDA was pinned. The option is to return an unreadable marker on decline.

**Checked clean**:
- Every `DeferredFragmentEffects` queue drains (67 fields).
- The `MovieEnded` resume order.
- The FORMAT bump for `PlayBink`.
- The #6512 cast changes.
- PlayBink and MoveToPackageLocation lowering.
- MoveTo with unloaded ends.
- Papyrus array bounds and `Int` wrapping.
- The new condition getters against geckwiki.
- FOREIGN_INDICES against xEdit.
