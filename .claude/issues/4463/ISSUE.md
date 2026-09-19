# D6-07: D6-07: feature-matrix regen row names "Health/Magicka/Stamina" — the built tick is Fatigue/Magicka only

- **Labels**: low,character,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4463

---

**Source**: `crates/core/src/character/regen.rs` (`PoolRegenConfig { fatigue_avif, magicka_avif, willpower_avif }`, `FATIGUE_REGEN_PER_SEC`, `magicka_regen_per_sec`); `docs/engine/charal.md` §4.7 ("Health… deliberately unmodelled").

**Description**

The feature-matrix row title at `docs/feature-matrix.md:265` reads "Pool regen tick (Health/Magicka/Stamina) | ✗ inert ×7" and the gap row (:341) says "passive Health/Magicka/Stamina regen … on all seven games". The shipped mechanism models Fatigue/Magicka (classic-Oblivion rates); Health is deliberately unmodelled (charal.md §4.7) and Stamina has no row.

**Evidence**

regen.rs struct/constants; charal.md §4.7 table. The row's body text (feature-matrix.md:300-305) is accurate about wiring — the misnomer is at label level.

**Impact**

The row is exactly what someone reads to scope "wire regen", and it implies Health/Stamina support that exists on no path. The inert ✗s currently mask it.

**Related**

#2950, #3483.

**Suggested Fix**

Relabel to "Pool regen tick (Fatigue/Magicka)" and, in the gap row, note Health/Stamina rates are unsourced/unmodelled per charal.md §4.7.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D6-07, /audit-character 2026-09-19, HEAD `479163836`).*