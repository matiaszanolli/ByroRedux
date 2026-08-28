# #3509: FO3-2026-08-27-D3-02: the XCLL_SIZES_FALLOUT_ERA comment inverts the FO3/FNV split — FO3 ships 40-byte XCLL 96% of the time

**Labels**: low, esm-plugin, documentation, doc-rot, game:fo3, legacy-compat
**Audit**: `docs/audits/AUDIT_FO3_2026-08-27.md`

---

Source: `docs/audits/AUDIT_FO3_2026-08-27.md` — finding `FO3-2026-08-27-D3-02` (LOW, Dimension 3 — doc-rot on a wire-layout constant).

## Location
`crates/plugin/src/esm/cell/walkers.rs` — the comment above `XCLL_SIZES_FALLOUT_ERA` (~L48-53)

## Description
```rust
// FO3 ships 36 bytes (dir_fade + fog_clip, NO fog_power); FNV adds the
// 4-byte Fog Power tail for 40. Both share `GameKind::Fallout3NV`, so the
// canonical set carries both — the size dispatch already parses 36 fine
// (per-field gating below), 36 was just missing here and tripped a spurious
// "lighting may be mis-computed" warn on every FO3 interior (D3-FO3-01).
const XCLL_SIZES_FALLOUT_ERA: &[usize] = &[28, 36, 40];
```

"FO3 ships 36 bytes … FNV adds the tail for 40" is backwards as a description of the corpus.

## Evidence
Raw sub-record size census over every CELL in both masters (measured this audit run):

```
FO3  XCLL: 40 B → 404,  36 B → 17
FNV  XCLL: 40 B → 388,  36 B →  0
```

FO3 ships the 40-byte form on 404 of 421 lit cells (95.9 %); the 36-byte form is a 17-record minority, not the FO3 norm. (The constant itself is right — both sizes belong in the set — and the size-based dispatch handles both. Only the rationale is wrong.)

## Impact
Doc-only today, but this comment is the stated justification for the contents of a size-dispatch table consumed by `xcll_canonical_sizes` / `xcll_size_sanity_warn`. A future change that "simplifies" it by gating 36 → FO3 and 40 → FNV, on the strength of this comment, would mis-parse 404 FO3 interiors' lighting. Same failure shape the comment itself was written to prevent.

## Related
The closed `D3-FO3-01` finding this comment cites.

## Suggested Fix
Restate as "FO3 authors both 36 and 40 (404 / 17 on `Fallout3.esm`); FNV authors 40 only" and keep the per-field gating rationale.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the neighbouring `XCLL_SIZES_*` per-era constants and their comments
- [ ] **TESTS**: A regression test pins this specific fix (`xcll_canonical_sizes` game→sizes map assertions already exist; keep them aligned with the corrected rationale)
