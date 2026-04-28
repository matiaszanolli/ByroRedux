# #759: SF-D5-03: `parse_rate_starfield` covers 1 of 5 vanilla mesh archives — Meshes02 0%, MeshesPatch 74%, others untested

URL: https://github.com/matiaszanolli/ByroRedux/issues/759
Labels: enhancement, nif-parser, medium

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 5, SF-D5-03)
**Severity**: MEDIUM (test coverage gap; defense-in-depth)
**Status**: NEW

## Description

`crates/nif/tests/common/mod.rs:101` exposes a single Starfield mesh archive:

```rust
pub fn mesh_archive(self) -> &'static str {
    match self {
        ...
        Game::Starfield => "Starfield - Meshes01.ba2",
    }
}
```

The "100% recoverable / 31058 NIFs" headline in ROADMAP and the audit-context bundle is real but underspecified — the corpus is ~89,000 NIFs across the five vanilla mesh archives, of which the gate test covers 35%.

## Per-archive results from the 2026-04-27 audit sweep

| Archive | NIFs | Clean % | Notes |
|---------|------|---------|-------|
| `Starfield - Meshes01.ba2` | 31 058 | 98.17% | covered by current test |
| `Starfield - Meshes02.ba2` | 7 552 | **0.00%** | uncovered; 100% hit by SF-D5-02 (`BSWeakReferenceNode`) |
| `Starfield - MeshesPatch.ba2` | 29 849 | 74.37% | uncovered; SF-D5-02 + #109 |
| `Starfield - LODMeshes.ba2` | 19 535 | 99.92% | uncovered |
| `Starfield - FaceMeshes.ba2` | 1 282 | 100.00% | uncovered |

Aggregate clean rate across the five archives: **~80.6%** (~71.7K clean / ~89.3K total).

## Suggested Fix

Two options:

1. **Extend `Game::Starfield`** to expose all five mesh archives (or split into `Game::Starfield`, `Game::StarfieldLOD`, etc.).
2. **Add a sibling `parse_rate_starfield_all_meshes` test** that walks the full vanilla mesh corpus, with per-archive thresholds.

Either way, document the per-archive clean rates in ROADMAP so the `Meshes02 = 0%` reality stops being hidden by the headline number.

## Completeness Checks

- [ ] **TESTS**: This issue *is* the test extension. Once SF-D5-02 (#754, `BSWeakReferenceNode`) lands, expect Meshes02 clean rate to climb out of 0%.
- [ ] **SIBLING**: Update ROADMAP per-game compat matrix with explicit per-archive numbers.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- SF-D5-02 (#754) — landing the BSWeakReferenceNode parser is what unblocks Meshes02 clean parse.
- SF-D1 regressions (#746, #747) — landing those bumps MeshesPatch from 74% upward.
