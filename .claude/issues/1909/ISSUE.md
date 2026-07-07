# #1909 — SCR-D5-NEW-05: rumble recognizer coerces a non-literal property to its .psc default instead of declining

_Filed from `docs/audits/AUDIT_SCRIPTING_2026-07-06.md`. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 1909 --json state`)._

---

**Severity**: LOW · **Dimension**: Recognizer-Chain Soundness · **Untrusted-Input**: No
**Location**: `crates/scripting/src/translate/recognizers/rumble.rs:37-41,67-75` (`float_prop`/`bool_prop` → `unwrap_or(default)`)
**Source**: audit `docs/audits/AUDIT_SCRIPTING_2026-07-06.md` (SCR-D5-NEW-05)

## Description
The checklist expects `rumble` to decline on a non-literal auto-property value; instead `float_prop`/`bool_prop` return `None` and the caller `unwrap_or`s the `.psc` default (`:37-41`) — a coercion, not a decline.

## Evidence
The extraction path (`float_prop` `:67`, `bool_prop` `:75`) returns `None` on a non-literal, and `recognize` `unwrap_or(d.*)`s each value rather than returning `None` from `recognize`.

## Impact
Harmless in practice — Papyrus auto-property initializers are literal-only (so the branch effectively fires only on an *absent* property), and the five extracted values are cosmetic rumble/shake tuning that don't change the behavior family. Logged only because it diverges from the stated "must decline, not coerce."

## Related
A defensible design choice for a cosmetic per-script recognizer; noted for invariant consistency.

## Suggested Fix
Either decline on a present-but-non-literal property value, or update the recognizer's contract note to document the intentional default-fallback for cosmetic parameters.

## Completeness Checks
- [ ] **DECLINE-INVARIANT**: Decision recorded — either decline on present-but-non-literal, or an explicit contract-note documenting the intentional default-fallback
- [ ] **TESTS**: If declining, a guard test pins that a non-literal property value declines; `recognizes_rumble_and_extracts_psc_defaults` still passes
