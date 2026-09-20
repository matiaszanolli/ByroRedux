# EXT-D3-2026-09-19-02: blade-arena doc rot — comments/design log say 16 MB / 4,096 blades; code allocates 64 MiB / 16,384

- **ID**: EXT-D3-2026-09-19-02
- **Labels**: low,terrain-exterior,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4497

**Severity**: LOW (doc rot) · **Dimension**: Ground cover · **Tier Violated**: no-fabrication (documentation asserts an allocation figure the code outgrew) · **Game Affected**: all (engine-level)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D3-2026-09-19-02; the 2026-09-19 seed, verified)

**Location**: `crates/renderer/src/vulkan/groundcover.rs:23-27` (module doc); `crates/renderer/src/shader_constants_data.rs:321-332` (`GROUNDCOVER_MAX_CHUNKS` doc — also cites a "2000-unit draw distance"; `GROUNDCOVER_DRAW_DISTANCE` is 3000); `docs/engine/exal-groundcover.md` §12.13

**Description**
`7996edf61` (2026-09-16) raised `GROUNDCOVER_CANDIDATES_PER_THREAD` 64→256, taking `GROUNDCOVER_MAX_BLADES_PER_CHUNK` 4,096→16,384 and the blade buffer 16→64 MiB. That commit correctly updated the size-pin test ("64 MB") and the `memory-budget.md` ledger row (67,690,884 B), but left three doc sites asserting the old figures (16 MB / 4,096), and §12.13 records only the first 4× rise. The module doc's "~50 chunks in view" vs the constants doc's "~67" shows the rot compounding.

**Impact**
No runtime effect (ledger, test pin, allocation consistent), but these are the "why sized this way" comments future tuners read — they understate a 4× VRAM commitment allocated on every RT device.

**Related**: `docs/engine/memory-budget.md` § Sky and Ground Cover (correct); #4338

**Suggested Fix**
Update both code comments to 64 MiB / 16,384 / draw distance 3000; append a dated §12.13 line recording the second 64→256 candidate rise.

## Completeness Checks
- [ ] **TESTS**: N/A (docs); the existing size-pin test already guards the code side
