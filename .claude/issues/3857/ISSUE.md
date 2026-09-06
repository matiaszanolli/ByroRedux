# #3857: TD1-2026-09-05-08: `asset_provider/material.rs` crossed 2044 production LOC because `merge_external_material` grew 37 % to 931 LOC since #2412 assessed it at 678 and recommended awareness only

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-08) via `/audit-publish`, 2026-09-05. Labels: `low,import-pipeline,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3857 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-08), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/asset_provider/material.rs` (2044 production / 2044 total — 100 % production); `merge_external_material` at `:1114`–`:2044`
- **Status**: NEW (not a regression of #2412 — that issue closed with an explicit *no-action* recommendation, so nothing was fixed to regress)
- **Age**: `b0a3fa02`, 2026-06-24; 1089 → 2044 across 38 commits. `merge_external_material` was
  **678 LOC on 2026-08-12** (#2412) and is **931 today** (+253, +37 % in 24 days)
- **Description**: #2412 measured this function at 678 LOC and concluded: *"No action recommended on
  `merge_external_material` beyond awareness — it is a deliberate single NIFAL boundary and should
  not be split in a way that weakens that invariant."* That reasoning was sound and is preserved by
  the fix below, but two things changed since: the function is now **the 5th-largest production
  function in the workspace** and **46 % of its host file**, and the host file itself has crossed
  the file-level threshold — which #2412 did not evaluate.
- **Evidence**: the function's control flow is a single three-way dispatch with two enormous arms:
  ```
  :1220   if starfield_cdb_gate && path.ends_with(".mat")   → apply_cdb_pbr_fallback, early return
  :1240-:1266  ~16 `let mut set_* = false` override sentinels
  :1295   if dispatch_kind == Some(MaterialKind::Bgsm)  { … }   ← ~456 LOC
  :1751   } else if dispatch_kind == Some(MaterialKind::Bgem) { … }  ← ~237 LOC
  :1988   } else { … }                                            ← ~38 LOC
  :2039   if touched { Merged } else { PresenceOnly }
  ```
  The rest of the file is already four separable clusters:
  - **BGSM/BGEM semantics helpers** — `forward_bgsm_phase1_flags`, `forward_bgsm_rim_subsurface`,
    `forward_bgsm_env_map_scale`, `conductor_diffuse_tint`, `bgsm_metalness`,
    `bgem_uses_glass_behavior`, `bgem_uses_thin_glass_behavior`, `bgsm_blend_to_gamebryo`;
  - **Starfield CDB** — `is_materialsbeta_cdb_path`, `SF_CDB_CACHE_MAX_ENTRIES`, `sf_cdb_cache`,
    `sf_cdb_cache_insert`, `discover_starfield_cdbs`, `cdb_scan_candidates`,
    `MaterialProvider::{register_starfield_cdb, register_starfield_cdb_probe, has_starfield_cdb}`,
    `apply_cdb_pbr_fallback`, `unresolved_material_warning`;
  - **provider + caches** — `build_material_provider`, `MaterialProvider`, `MAX_BGEM_CACHE_ENTRIES`,
    `MAX_FAILED_PATHS`, `new`, `geometry_csg`, `push_archive`, `extract_from_archives`,
    `resolve_bgsm`, `resolve_bgem`, `peek_magic`, `insert_{bgsm,bgem}_for_test`;
  - **the merge boundary** — `MergeOutcome`, `record_external_texture_sources`,
    `merge_external_material`.

  Supporting nit (cross-refer **Dimension 2**): the LRU half-eviction body
  `if len >= MAX { for _ in 0..MAX/2 { if let Some(old) = …_order.pop_front() { …remove(&old) } } }`
  is written out four times (`:735`, `:770`, `:859`, `:875`) and produces the file's only
  nesting-depth-> 5 site (`:736`, seven levels inside `resolve_bgsm`). One
  `half_evict(order, set, cap)` helper removes all four.
- **Impact**: this is the single NIFAL `ImportedMaterial` sidecar boundary — every FO4/FO76/Starfield
  material finding routes through it, and per `_audit-severity.md` a wrong translation here is HIGH
  by construction with no per-draw fallback to mask it. A 931-LOC function is a poor place to keep
  that invariant reviewable.
- **Related**: #2412 (CLOSED, awareness-only, at 678 LOC); #2709 (`MergeOutcome`, the tri-state
  return this function grew to carry); #2702 (the mirror-test defect that motivated extracting
  `forward_bgsm_*` out of this same loop — the precedent for the fix below); `/audit-nifal` owns
  correctness.
- **Suggested Fix**: **preserve #2412's invariant explicitly** — `merge_external_material` stays the
  one public entry point and the one place a sidecar can touch `&mut ImportedMaterial`. Extract only
  its two arms into private siblings, `merge_bgsm_arm(&mut ImportedMaterial, &ResolvedMaterial, &mut MergeSentinels, …)`
  and `merge_bgem_arm(…)`, with the ~16 `set_*` bools promoted to a `MergeSentinels` struct. That is
  the same extraction #2702 already performed for `forward_bgsm_phase1_flags` and for the same
  stated reason (tests reach the real logic). At file level, split
  `asset_provider/material/{mod,cdb,provider,merge}.rs`.
- **Effort**: medium

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
