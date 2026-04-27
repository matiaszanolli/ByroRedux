# NIF-D5-08: Starfield BoneTranslations block undispatched

URL: https://github.com/matiaszanolli/ByroRedux/issues/726
Labels: enhancement, nif-parser, low

---

## Severity: LOW

## Game Affected
Starfield

## Location
- `crates/nif/src/blocks/mod.rs` — no dispatch arm

## Description
Not in nif.xml (Starfield-era addition). Appears only in skinned Starfield meshes, paired with `SkinAttach` and `BSGeometry`. Likely supplies per-bone translation offsets (Starfield's modular character creator).

## Evidence
2026-04-26 corpus sweep:
- `Starfield - Meshes01.ba2` — 154 occurrences

## Impact
Modular character bone offsets lost; partial unblock for fully procedural Starfield characters.

## Suggested Fix
Disassemble alongside #708 (NIF-D5-01 BSGeometry) and #709 (NIF-D5-02 SkinAttach) — the three blocks form the SF skinned-geometry triple.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-08)
- Bundle: #708 BSGeometry, #709 SkinAttach

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Bundle with #708 + #709 — same SF wire-format reverse-engineering session
- [ ] **TESTS**: Byte-exact dispatch test once layout is RE'd
