# Incremental / Delta Audit — 2026-07-05

**Scope:** last 10 commits, `HEAD~10..HEAD` (HEAD = `a8d65d6c`).
**Method:** per-`_audit-incremental` — route each changed file to its owning
audit dimension, apply that dimension's checks to the diff only, re-read the
current symbol to confirm each candidate, attempt to disprove, then dedup.

## 1. Change summary

| Commit | Issues | Theme |
|--------|--------|-------|
| `a8d65d6c` | #1889 | Materialise VWD header flag as a per-placement `VisibleWhenDistant` marker |
| `8b50e238` | #1840/#1841 | Delete the last 7 call-site-less `NifVariant` helpers + regen 5 per-block baselines |
| `aedcba12` | #1836/#1837 | Name the poisoned lock in `clear_entities` / `insert_resource` |
| `db121f96` | #1834/#1835 | Save `ActorValues`; add NPC-spawn-stamp save-gap tripwire test |
| `155852e3` | #1885 | Route `NiBlendInterpolator` blend-array counts through `allocate_vec` |
| `88d41600` | #1850/#1851 | Surface dropped breakable-ragdoll edges + pin measured joint counts |
| `450691e0` | #1838/#1839 | Restore raw-BSVER version gates at four NIF sites |
| `7e6122c4` | — | Correct three stale FO3-audit comments (NO_LIGHTING, XATO, FO3 baseline) |
| `07f4b1b8` / `82921415` | — | Add NIF + tech-debt audit docs; fix per-block coverage-gate blind spot |

Character of the delta: overwhelmingly **hardening + tests + doc/comment
corrections + dead-code removal**. One behavioural feature (the VWD marker) and
one behavioural change (fail-loud on poisoned resource lock). No renderer
pipeline/barrier/AS changes; the single shader touch (`triangle.frag`) is
**comment-only** (no SPIR-V recompile needed, verified: the hunk is entirely
`//` lines).

## 2. Routing map

| Changed file | Dimension(s) | Result |
|--------------|--------------|--------|
| `crates/core/src/ecs/world.rs` (poison naming, insert_resource) | `/audit-ecs`, `/audit-concurrency` | Clean — disjoint field borrows OK, no lock-order change |
| `crates/save/src/registry.rs`, `byroredux/src/save_io.rs` (#1834/#1835) | `/audit-save` | Clean — `ActorValues` delta-safe; allowlist accurate (see §3) |
| `crates/core/src/ecs/components/actor_values.rs` | `/audit-save`, `/audit-ecs` | Clean — serde derives gated behind `inspect` feature |
| `crates/nif/src/blocks/{interpolator,node,tri_shape/ni_tri_shape}.rs`, `collision/shape_compound.rs` (#1838/#1839/#1885) | `/audit-nif`, per-game | Clean — gates match nif.xml; `allocate_vec` guard sound |
| `crates/nif/src/version.rs` (dead-helper removal) | `/audit-nif` | Clean — 0 residual live call sites (see §3) |
| `crates/nif/src/import/collision/ragdoll.rs` (#1850/#1851) | `/audit-safety`, per-game | Clean — pure helper, log-only, no fabrication |
| `crates/plugin/src/esm/cell/{mod,support,walkers}.rs`, `records/grup_walker.rs` | per-game, `/audit-legacy-compat` | VWD plumbing clean; XATO caveat = Existing #1887 |
| `byroredux/src/components.rs`, `cell_loader/references/mod.rs`, `cell_loader/object_lod.rs` (#1889) | per-game, `/audit-ecs` | Marker has no consumer by design (see F1) |
| `crates/renderer/shaders/triangle.frag` | `/audit-renderer` | Comment-only; no functional/struct change |
| `crates/nif/tests/**`, `**/*_tests.rs`, baselines `*.tsv` | `/audit-regression` | Baselines internally consistent (see §3) |
| `docs/**`, `.claude/issues/**` | `/audit-tech-debt` | Doc/tracking only |

## 3. Disproof log (candidate findings that did NOT survive)

- **#1885 `allocate_vec` spurious rejection?** — Disproved. `allocate_vec`
  compares element `count` against **bytes remaining** with a 1-byte-per-element
  floor (`stream.rs:253`), not `count * size_of`. Each `InterpBlendItem` reads
  ≥17 wire bytes, so a valid file's count is always ≤ remaining bytes; the guard
  only rejects corrupt over-counts. Reassigning `items` in the
  manager-controlled `parse_modern` arm is exact (`items` was empty). Sound.
- **#1838/#1839 wrong BSVER gate?** — Disproved. `> FO3_FNV` (=34, `#BS_GT_FO3#`)
  and `>= SKYRIM_LE` (=83, `#BS_GTE_SKY#`) both match nif.xml and are the correct
  fix for the 35..=82 `Unknown`-variant corner the `variant()` helpers misfired
  on. Both constants exist (`version.rs:335,341`).
- **#1840 dead-helper removal left a live caller?** — Disproved.
  `grep` for `has_shader_alpha_refs|has_culling_mode|has_material_crc` across
  `crates/` + `byroredux/` returns **only comments/test-doc strings** — zero live
  call sites. `has_shader_property_fo3_fields` (the sole survivor) still has its
  `shader_flags.rs` consumer.
- **Per-block baseline regen masking a regression?** — Disproved. Totals are
  conserved: FO3 `NiPSysBlock 5525→4041` exactly equals the sum of the newly
  resolved `BSPSysSimpleColorModifier 419 + NiPSysEmitter 422 + NiPSysEmitterCtlr
  361 + NiPSysGrowFadeModifier 282 = 1484`. Every `unknown` column stays `0`.
  Legitimate alias-resolution improvement, not a coverage loss.
- **#1834 allowlist claim false (silent save gap)?** — Disproved. `grep` for
  runtime mutators of `Perks`/`CharacterLevel`/`Background`/`FactionRanks`
  (`query_mut`, `get_mut`, `add_perk`, `set_level`, `gain_xp`, …) finds **none**
  outside the save_io comment itself. The REDERIVED_NOT_SAVED allowlist is
  accurate for current state.
- **`clear_entities` disjoint-borrow / `insert_resource` panic UB?** — Disproved.
  `storages.iter_mut()` + `type_names.get()` are disjoint field borrows (compiles;
  full suite green). Panic-on-poison is the deliberate #466 fail-fast doctrine,
  mirroring `remove_resource`; the fresh lock is installed before the panic path.
- **#1850 breakable-ragdoll double-processing / OOB index?** — Disproved.
  `BhkBreakableConstraint` fails the `BhkConstraint` downcast → enters the `else`
  arm once → warns → `continue`. `bodies[body_a]` indices come from
  `block_to_body` values (always valid body indices). Log-only, no state change.

## 4. Findings

### DELTA-01: `VisibleWhenDistant` marker is inserted but has no reader
- **Severity**: LOW
- **Dimension**: tech-debt / ECS
- **Location**: `byroredux/src/components.rs:126-129`, `byroredux/src/cell_loader/references/mod.rs:750-757`
- **Status**: NEW
- **Changed in**: commit `a8d65d6c` (#1889)
- **Description**: The new `VisibleWhenDistant` component is stamped on every
  VWD-flagged placement root at cell load but is never queried anywhere — a
  write-only marker. This is explicitly documented as intentional (a parse→spawn
  hook the deferred full-model LOD cull will read once the full-detail radius is
  decoupled from the streaming ring; the conservative ring currently makes an
  active cull unnecessary).
- **Impact**: Negligible — one `SparseSetStorage` entry per VWD placement, no
  correctness effect. Recorded only so the write-only state is tracked, not
  mistaken for an oversight, and revisited if the cull work is dropped.
- **Related**: EXAL §5.2; #1731 (flag parse), #1866 (ring hysteresis)
- **Suggested Fix**: None now. When the full-model cull lands, wire a reader in
  the streaming/visibility path; if that work is abandoned, delete the marker.

### DELTA-02: `insert_resource` doc comment omits the new panic-on-poison contract
- **Severity**: LOW
- **Dimension**: documentation
- **Location**: `crates/core/src/ecs/world.rs:556-557`
- **Status**: NEW
- **Changed in**: commit `aedcba12` (#1837)
- **Description**: `insert_resource` now re-panics (via `resource_lock_poisoned::<R>()`)
  when the prior value's lock was poisoned, changing observable behaviour from a
  swallowed `None`. The public doc line still reads only "Returns the previous
  value if one existed" with no mention that a poisoned prior lock panics.
  (Consistent with the rest of the crate — `remove_resource` etc. don't document
  it either — so this is a hygiene note, not a defect.)
- **Impact**: None at runtime; a caller reading the rustdoc won't learn the
  fail-fast behaviour.
- **Suggested Fix**: Add a one-line `# Panics` note, ideally crate-wide on the
  poison-propagating resource methods, not just here.

### DELTA-03: XATO alt-TXST is mis-read on FONV (documented, deferred)
- **Severity**: MEDIUM
- **Dimension**: legacy-compat (per-game FNV/FO4)
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:938-960`
- **Status**: Existing #1887 (local tracked issue; behaviour unchanged this range)
- **Changed in**: commit `7e6122c4` (comment/provenance only)
- **Description**: `parse_refr_group` is not `GameKind`-gated, so a FONV REFR's
  `XATO` (Activation-Prompt string, grouped with SCRV/SCVR/SLSD) has its first
  4 bytes read as a spurious alt-TXST FormID. This commit only *documents* the
  caveat; the parse behaviour is pre-existing and unchanged.
- **Impact**: Near-certain `texture_sets` miss → inert empty overlay (no visual
  effect, one wasted alloc). Same caveat flagged for XTNM/XTXR.
- **Related**: FO4-DIM6-02 / #584 (the FO4 origin), FO3-D3-001 / #1887
- **Suggested Fix**: Thread `GameKind` through `parse_refr_group` and gate the
  XATO/XTNM/XTXR arms to FO4+; re-validate the FO4 overlay fixtures. Tracked in
  #1887 — no new issue.

## 5. Missing tests

- **VWD positive-path spawn test (#1889)** — LOW. The flag *reader*
  (`RecordHeader::is_visible_when_distant`) is tested in `reader.rs`, but no test
  asserts that a VWD-flagged base record propagates
  `StaticObject::visible_when_distant = true` through `build_static_object_from_subs`
  → `load_references` → a `VisibleWhenDistant` marker on the spawned root. Every
  existing `build_static_object_from_subs` call passes `false`. A single positive
  assertion would close the parse→spawn plumbing the marker exists to preserve.

## 6. Verdict

Clean delta. The 10-commit window is hardening, tests, dead-code removal, and
comment/baseline corrections; the two behavioural changes (VWD marker, fail-loud
resource-lock poison) are correct and well-tested. **No CRITICAL or HIGH
regressions.** Every version-gate, allocation-guard, dead-helper-removal, save
round-trip, and baseline-regen candidate was disproved on re-read. Residual
findings are 2× LOW (intentional write-only marker; a `# Panics` doc gap) and
1× pre-existing tracked MEDIUM (#1887 XATO, only re-documented here). The #1889
fix audited skeptically holds up.

---

```
/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-07-05.md
```
