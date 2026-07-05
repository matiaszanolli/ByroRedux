**Severity**: LOW · **Dimension**: ESM Record Coverage · **Game**: FO3 (premise refutation); FNV (benign misparse, shared parser)
**Source**: `docs/audits/AUDIT_FO3_2026-07-05.md` (FO3-D3-001)

## Description
The `/audit-fo3` premise that FO3 REFRs carry XATO/XTNM/XTXR texture overrides (#584) is **incorrect**. openmw's authoritative `esm4/loadrefr.cpp` tags `XATO` as `// FONV` (New-Vegas-only) and *skips* it — it is the Activation-Prompt subrecord, not a TXST FormID. FO3 REFRs never carry XATO. The `b"XATO"` arm in `crates/plugin/src/esm/cell/walkers.rs` has no game gate and reads 4 raw bytes as a FormID unconditionally; the adjacent comment claiming it holds "the 140 MNAM-only vanilla FO4 TXSTs" is unsourced and does not match the openmw provenance.

## Evidence
- `reference/openmw/components/esm4/loadrefr.cpp` — `case ESM::fourCC("XATO"): // FONV` (in the skip list, `reader.skipSubRecordData()`); the FO3-tagged REFR subrecords (RCLR/XRDO/SCRO/…) carry no XATO.
- `crates/plugin/src/esm/cell/walkers.rs` — `b"XATO" => { alt_texture_ref = r.u32().ok(); }` reads a u32 FormID with no `game`/`version` gate.
- `byroredux/src/cell_loader/refr.rs::build_refr_texture_overlay` — any non-`None` `alt_texture_ref` forces a `Some(overlay)`; the FormID is looked up in `index.texture_sets` (a miss contributes nothing).

## Impact
- **FO3**: zero — the arm never fires (XATO absent from FO3 REFRs). #584's overlay system is FO4-scoped, not FO3-scoped.
- **FNV** (shared parser): a REFR's XATO (activation-prompt string) has its first 4 bytes read as a u32 FormID, near-certainly missing `texture_sets` (activation-prompt ASCII → high mod-index byte, far outside any real load order), yielding a spurious empty `Some(overlay)` + a wasted per-REFR allocation instead of `None`. Functionally inert (base-mesh textures ride through per the overlay tests), but a misparse.

## Related
#584 (REFR texture overrides), #1654. Overlay tests (`byroredux/src/cell_loader/refr_texture_overlay_tests.rs`) are entirely FO4-authored — no FO3/FNV fixture exercises the arm.

## Suggested Fix
Either (a) game-gate the XATO/XTNM/XTXR arms to FO4+ so FNV activation-prompt strings aren't mis-read into `alt_texture_ref`, or (b) at minimum correct the `walkers.rs` comment to cite the real provenance (XATO = FONV REFR Activation-Prompt subrecord, not "FO4 TXST"). Low urgency because the effect is inert.

## Completeness Checks
- [ ] **SIBLING**: If gating XATO, check the XTNM/XTXR sibling arms in the same walker take the same game gate
- [ ] **CANONICAL-BOUNDARY**: The resolved-path override feeds `translate_material` via `ResolvedPaths` — keep per-game gating at the parse/walker boundary, not in the renderer
- [ ] **TESTS**: Add an FNV/FO3 fixture (or a negative test) so the arm's game-scope is pinned
