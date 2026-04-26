# Issue #684: O5-1: ROADMAP and CLAUDE.md claim 100% Oblivion parse rate; measured rate is 95.21%

**Severity**: CRITICAL (doc-truth bug)
**Files**: `ROADMAP.md:71, 102`, `CLAUDE.md` (Session 12 note), `crates/nif/examples/nif_stats.rs:50`
**Dimension**: Real-Data Validation

`ROADMAP.md:71` lists `Oblivion | BSA v103 | 100% (8 032)`. `CLAUDE.md` says "Full-archive parse rates: ALL 7 games at 100% (177,286 NIFs)."

Real measured clean-parse rate on `Oblivion - Meshes.bsa` (2026-04-25, commit 1ebdd0d) is **7,647 / 8,032 = 95.21%**, with 384 truncated and 1 hard fail. Run time 2.21s release on Ryzen 7950X.

This regression has persisted across the 04-17 → 04-25 window despite the H-1 parser additions; the truncation reasons shifted (NiNode-root → particle modifiers / NiTransformData drift) but the file count only dropped from ~678 to 384.

**Top truncation reason histogram**:
| Count | Reason |
|---|---|
| 154 | "failed to fill whole buffer" (root NiNode size-walk underrun) |
| 84 | "exceeds hard cap" allocation rejects (corrupt count harvested via drift) |
| 68 | "unknown KeyType" on NiTransformData / NiPosData / NiFloatData |
| 18 | bogus "X-byte read at position Y, only Z remaining" (post-drift symptom) |

**`nif_stats` exit code is 1 (gate firing correctly)**, so this is a doc-truth bug, not a behavior regression. The actual parse-rate recovery work is tracked separately in O5-2 + O5-3.

**Fix**:
- Update `ROADMAP.md:71` to "95.21% (7647/8032)".
- Update `ROADMAP.md:102` similar.
- Update `CLAUDE.md` "ALL 7 games at 100%" claim.
- Re-measure FNV/FO3/FO4/Skyrim/FO76/Starfield with the same methodology to confirm whether the "100%" claim is also stale on those games.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
