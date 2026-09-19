# FO3-D4-2026-09-19-01: FO3-D4-2026-09-19-01: FO3 DLC object-LOD `.high.` quad variant is shipped but never consumed (fidelity-only; no coverage hole)

- **Labels**: low,bug,terrain-exterior,game:fo3,legacy-compat
- **Filed from**: docs/audits/AUDIT_FO3_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4468

---

**Dimension**: 4 — FO3 Cell Loading
**Filed from**: `docs/audits/AUDIT_FO3_2026-09-19.md` (/audit-fo3, HEAD `340799d66`)

**Description**

FO3's DLC mesh archives ship a second object-LOD filename variant, `<world>.level4.high.x<X>.y<Y>.nif`, beside the plain form. Raw census of the 5 DLC `*- Main.bsa` archives: **60 `.high.` quads** — `dlc02anchoragebattle` 12, `dlc02chinesehq` 13, `dlc02glacier` 21, `dlc02overlook` 11, `tlandscape` 3 (the latter a base-game test worldspace whose LOD ships only in `Anchorage - Main.bsa`). No code path generates or probes a `.high.` path (repo-wide grep: zero consumers; `object_lod_archive_path` at `byroredux/src/cell_loader/object_lod.rs:674-690` builds only the plain `{w}.level{L}.x{X}.y{Y}.nif` form).

**Evidence**: every one of the 60 `.high.` coordinates has a plain sibling in the same archive (verified set-difference = 0), so the plain-form descent still finds a quad everywhere the DLC bakes one — detail fidelity only, no hole, no mis-placement. Also a census gap: `object_lod.rs`'s archive inventory comment (`:626-647`) counts only the base BSA ("blocks across 15 worldspaces"), so the DLC worldspace quads — and the variant — are absent from the documented inventory a future auditor would start from. No source was available to confirm vanilla FO3 actually prefers `.high.` at any LOD setting, so this is recorded as an unconsumed shipped asset + census gap, not a behavioral regression.

**Impact**: at most a detail-fidelity difference on Anchorage exteriors (the engine always uses the lower-density plain quad where a higher-detail one exists).

**Related**: #2086 (closed — the "FO3/FNV ship no object LOD" premise), #3321 (closed — FalloutLegacyBlocks scheme), #3502 (closed — coarsen escape).

**Suggested Fix**: either extend the census comment to record the DLC counts and the `.high.` variant (cheap, honest), or probe the `.high.` form as a preferred-then-plain fallback in `object_lod_archive_path`'s consumer with a cited source for vanilla's selection rule. If neither is warranted, note the variant as deliberately out of scope beside the scheme docs.

## Completeness Checks
- [ ] **SIBLING**: Check FNV's DLC archives for the same `.high.` variant while touching the census
- [ ] **TESTS**: If the fallback lands, a fixture pins preferred-then-plain selection; if only the census comment changes, a reviewer verifies the counts
