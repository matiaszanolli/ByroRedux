## Finding NIF-NEW-04 — NIF Audit 2026-06-13

- **Severity**: MEDIUM
- **Dimension**: Stream Position
- **Game Affected**: Oblivion only (sizeless format).
- **Location**: `crates/nif/src/blocks/controller/morph.rs` — `NiMorphData::parse` (line 145, plausibility guard 156-160) and `NiGeomMorpherController` (same file); surfaces in `crates/nif/src/lib.rs::parse_nif`.
- **Status**: NEW — validated CONFIRMED at HEAD `8d191d7d`.

## Description

`NiMorphData` parses then trips its own num_morphs/num_vertices plausibility guard, or `NiGeomMorpherController` fails "failed to fill whole buffer" — stream drift caught by sanity guards rather than the alloc cap. Sizeless format → truncate.

## Evidence (validated)

- `NiMorphData::parse` at `morph.rs:145`; reads `num_morphs`/`num_vertices` as u32 then the implausibility guard at `:156-160` (`if num_morphs > 65_536 || num_vertices > 65_536 { … "NiMorphData: implausible num_morphs={num_morphs} …" }`).
- `sweep_oblivion.err`: `NiMorphData: implausible num_morphs=1535393654 num_vertices=568017444 — DISCARDING 10 blocks`; `NiGeomMorpherController (consumed 3366): failed to fill whole buffer — DISCARDING 12 blocks`. 8 morph-data + 1 morpher-controller failures.

## Impact

~9 of 56 truncated Oblivion scenes — creature facial morph rigs (goblinhead, minotaur head, doghead, mountainlion). The plausibility guard is doing its job (catching the drift, not causing it); the fix is upstream stride correctness. Drops morph targets + trailing blocks.

## Suggested Fix

Byte-audit `NiMorphData` (morph-key array stride, per-frame key layout) and `NiGeomMorpherController` against nif.xml for the Oblivion version band. Likely shares the early-sub-version comparator need with NIF-NEW-01/02/03.

## Completeness Checks
- [ ] **UNSAFE**: N/A expected
- [ ] **SIBLING**: Check the morph-key interpolation stride matches the other Oblivion-era keyed-data parsers (NiKeyframeData / NiColorData per-frame layout)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **CANONICAL-BOUNDARY**: N/A (parse-layer)
- [ ] **TESTS**: Add an Oblivion NiMorphData round-trip fixture; regression check = the creature-head scenes parse with 0 dropped blocks

---
Source: `docs/audits/AUDIT_NIF_2026-06-13.md` · Filed by `/audit-publish` · NIF-D3-NEW-10
