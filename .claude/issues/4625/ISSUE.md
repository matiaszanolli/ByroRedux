# NIF-D1-2026-09-21-01: Pre-Gamebryo BoundingVolume arm table is incomplete now that #4150 makes unlisted types a hard Err

**Issue**: #4625
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: LOW
**Dimension**: 1 (Stream Position Integrity)
**Location**: `crates/nif/src/blocks/base.rs:318-367`
**Status**: NEW (a side effect of closed #4150)

## Description
Pre-Gamebryo `BoundingVolume` arm table is incomplete now that #4150 makes an unlisted type a hard `Err`.
- `BASE_BV = 0xFFFFFFFF` is a nif.xml-legal value with no body (nif.xml:1279-1286, 2546-2553), and `read_and_skip_bounding_volume`'s `_` arm now errors on it instead of no-op-skipping.
- OpenMW's Morrowind reader (`components/nif/node.cpp:75-127`) also reads `LOZENGE_BV = 3`, which Gamebryo 3.2 reserves too; the Rust match here has no `3 =>` arm either (only 0, 1, 2, 4, 5 are handled — confirmed at HEAD `ee6d3fb39`).
- OpenMW gates the half-space BV's 12-byte origin on version ≥ 4.2.1.0; the Rust reader always skips 28 bytes unconditionally.
- Only v ≤ 4.2.2.0 content reaches this code, and none of the seven target titles (Oblivion is v20.0.0.4+) ship it — so this is currently dormant, but #4150's hard-Err means any pre-Gamebryo fixture or future Morrowind-era work would now fail outright instead of degrading.

## Evidence
`crates/nif/src/blocks/base.rs:318-367` — `read_and_skip_bounding_volume`'s `match bv_type` covers `0, 1, 2, 4, 5` only; `_ =>` returns `Err` (per #4150). Confirmed unchanged at HEAD `ee6d3fb39`.

## Impact
No effect on any currently-supported title's vanilla or mod content; a latent gap that would surface only if pre-4.2.2.0 (pre-Gamebryo NetImmerse) content is ever targeted.

## Related
#4150 (closed — the fix whose hard-Err exposed this gap)

## Suggested Fix
Add `3 => { /* LOZENGE_BV */ }` and a version-gated half-space origin skip if/when pre-Gamebryo content is in scope; otherwise document the version floor explicitly as a known, accepted gap.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D1-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix, if implemented
