# Incremental / Delta Audit — 2026-07-06

**Scope:** last 10 commits, `git diff HEAD~10..HEAD` = `db121f96..a0b452d6`
(the 10 commits `aedcba12` → `a0b452d6`).

**Method:** every changed path routed to its owning audit dimension per
`audit-incremental/SKILL.md` Step 2, then the diff hunks audited against that
dimension's Step-3 regression checks. Each candidate finding was re-read and an
attempt made to disprove it. Deduped against 42 open issues
(`/tmp/audit/incr-issues.json`) and `docs/audits/`. NIF-parser hunks were
cross-checked against the authoritative nif.xml
(`/mnt/data/src/reference/nifxml/nif.xml`).

> This report supersedes the earlier same-day file (which covered the partly
> overlapping window `155852e3..d59f40ac`). The genuinely-new material in this
> window vs. that report is commit `b20e4863` — the FO76 particle-field
> consumption, header string-alloc bound, and the exact-`FLT_MAX` backlight
> gate — which the earlier report predated. The two commits that fell out of
> the `HEAD~10` window (`155852e3` #1885 NiBlendInterpolator alloc,
> `db121f96` #1834/#1835 save-registry) were audited clean in that prior run
> and are recoverable from git (`a0b452d6`).

---

## 1. Change summary

| Commit | Theme | Risk area |
|--------|-------|-----------|
| `aedcba12` | #1836/#1837 — name the poisoned lock in `clear_entities` + fail-fast `insert_resource` | ECS |
| `8b50e238` | #1840/#1841 — delete 7 dead `NifVariant` predicates + regen 5 baselines | NIF parser |
| `a8d65d6c` | #1889 — materialise base-record VWD flag as per-placement `VisibleWhenDistant` marker | ESM / cell loader / EXAL |
| `d4b981fa` | #1890/#1891 — pin the VWD spawn plumbing + document resource poison panics | ECS / cell loader |
| `196169a8` | #1874 — add `DBG_VIZ_MOTION` debug view (live ghosting root-cause) | Renderer (shader) |
| `42adc1e6` | #1892/#1893 — sync stale RT/denoiser docs with live pipeline | docs (renderer) |
| `e4d574dc` | #1894/#1895 — correct stale depth + SVGF docstring facts | docs (renderer) |
| `d59f40ac` | add 2026-07-05 performance + renderer audit reports | docs |
| `b20e4863` | #1903/#1896/#1901 — bound header string alloc + consume FO76 `#BS_F76#` particle fields + exact `FLT_MAX` backlight gate | NIF parser |
| `a0b452d6` | add 2026-07-05..06 NIF + safety audit reports | docs |

**Themes:** hardening + doc-sync + closing prior audit findings — **no new
feature work.** Every code hunk is a defensive fix (fail-fast poison naming,
header allocation budget guard, FO76 field alignment, tightened numeric gate),
a dead-code deletion, a per-record signal materialisation with no runtime
consumer yet, or a diagnostic-only debug view.

### Changed files (code, excluding docs / issue-tracking / baselines / binaries)

- `crates/core/src/ecs/world.rs` — poison-lock naming + fail-fast `insert_resource`
- `crates/core/src/ecs/world_tests.rs` — 2 new poison-panic tests
- `crates/nif/src/blocks/particle.rs` — 3 FO76 `#BS_F76#` field-consumption sites + 2 tests
- `crates/nif/src/blocks/shader.rs` — backlight gate `3.0e38` → `f32::MAX`
- `crates/nif/src/header.rs` — new `check_header_alloc` bound + 2 tests
- `crates/nif/src/version.rs` — delete 7 call-site-less `NifVariant` predicates + their tests
- `crates/nif/src/blocks/dispatch_tests/nodes.rs`, `tri_shape_*_tests.rs`, `collision/shape_compound_tests.rs` — drop deleted-helper assertions
- `crates/plugin/src/esm/cell/mod.rs`, `support.rs`, `records/grup_walker.rs` — thread `visible_when_distant` through `build_static_object_from_subs`
- `crates/plugin/src/esm/cell/tests/addn_stat.rs` — new VWD-flag parse tests
- `byroredux/src/components.rs` — new `VisibleWhenDistant` marker
- `byroredux/src/cell_loader/references/mod.rs` — `stamp_visible_when_distant` + test
- `byroredux/src/cell_loader/object_lod.rs` — comment only
- `byroredux/src/cell_loader/{pkin,scol}_expansion_tests.rs`, `esm/cell/tests/merge.rs` — new struct field in fixtures
- `crates/renderer/build.rs`, `shaders/include/shader_constants.glsl`, `shaders/triangle.frag`, `src/shader_constants_data.rs`, `triangle.frag.spv` — `DBG_VIZ_MOTION` (0x20000) debug view
- `crates/renderer/src/vulkan/acceleration/constants.rs` — comment only (ALLOW_COMPACTION rationale)
- `crates/renderer/src/vulkan/svgf.rs` — doc comment only (à-trous pass + format)
- `crates/bsa/examples/obl_sweep.rs` — new throwaway audit example (not a test)

---

## 2. Routing map

| Changed path | Routed dimension(s) | Verdict |
|--------------|---------------------|---------|
| `crates/core/src/ecs/world.rs` (+tests) | `/audit-ecs`, `/audit-concurrency` | Clean — see §3 |
| `crates/nif/src/blocks/particle.rs` (+tests) | `/audit-nif`, per-game FO76 corner | Clean — nif.xml-faithful |
| `crates/nif/src/blocks/shader.rs` | `/audit-nif`, `/audit-fo4` | Clean — matches `#FLT_MAX#` |
| `crates/nif/src/header.rs` (+tests) | `/audit-nif`, `/audit-safety` | Clean — OOM guard correct |
| `crates/nif/src/version.rs` (+ dispatch/tri_shape/collision tests) | `/audit-nif` | Clean — no live call sites |
| `crates/plugin/src/esm/cell/**`, `records/grup_walker.rs` (+tests) | per-game `/audit-<game>`, `/audit-legacy-compat` | Clean — all call sites updated |
| `byroredux/src/components.rs`, `cell_loader/references/mod.rs`, `object_lod.rs` | `/audit-nifal` (EXAL mirror), per-game | Clean — signal-only, no consumer |
| `crates/renderer/build.rs`, `shaders/**`, `shader_constants_data.rs`, `triangle.frag.spv` | `/audit-renderer` (+ define lockstep) | Clean — value 0x20000 lockstep across all 4 sites |
| `crates/renderer/src/vulkan/acceleration/constants.rs`, `svgf.rs` | `/audit-renderer` | Clean — comment/doc only, verified true |
| `crates/bsa/examples/obl_sweep.rs` | `/audit-tech-debt` | Throwaway tool — no finding |
| `docs/**`, `.claude/issues/**`, `*.tsv` baselines | `/audit-tech-debt` (doc rot) / `/audit-regression` | Out of code scope |

---

## 3. Findings

**No CRITICAL / HIGH / MEDIUM / LOW severity findings.**

This delta is a set of careful fix-issue closeouts; every code path was
re-audited against its dimension's checks and each candidate concern disproved.
The verification evidence, path by path:

### ECS — `world.rs` poison-lock fail-fast (`aedcba12`)
- `clear_entities` now resolves the storage type name via the #466 `type_names`
  side-table and calls `storage_lock_poisoned_erased`; `insert_resource` now
  re-panics on a poisoned prior-value lock (via `resource_lock_poisoned::<R>()`)
  instead of `.ok()`-swallowing it into `None`. Both helper fns confirmed to
  exist (`world.rs:34`, `world.rs:45`).
- **Contract / behavior delta:** `insert_resource` signature unchanged; the only
  behavioral difference is in the *poisoned-lock* path (panic vs `None`), which
  is the documented #466 fail-fast doctrine, not a regression. The
  `.expect("resource type mismatch")` downcast is statically unreachable (key is
  `TypeId::of::<R>()`, value inserted as `R`).
- **Lock/query delta:** no RwLock scope or acquisition-order change — the loop
  in `clear_entities` still iterates `storages` once, `get_mut()` on each. No
  deadlock surface introduced.
- Both new paths are pinned by tests
  (`insert_resource_over_poisoned_lock_panics_with_type_name`,
  `clear_entities_poisoned_lock_panics_with_type_name`).

### NIF — FO76 `#BS_F76#` particle fields (`b20e4863`)
- All three sites gate on `stream.bsver() == bsver::FO76` (== 155). Verified
  against nif.xml line 29: `<verexpr token="#BS_F76#" string="(#BSVER# #EQ# 155)">`
  — an **exact** equality, so `==` (not `>=`) is the faithful gate.
- The interleaved rotation-modifier fields (Vector4 + byte = 17 B) are nif.xml
  4874–4875 (`vercond="#BS_F76#"`); the NiPSysData Vector3 (12 B) is nif.xml
  4031; the `BSPSysSimpleColorModifier` `Unknown Shorts[26]` (52 B) tail — all
  three match. The rotation split's byte arithmetic checks out: the non-FO76
  path is `skip(4)+skip(8)=12` (identical to the old `skip(12)`), the FO76 path
  adds the 17 B between them. Test
  `parse_rotation_modifier_reads_fo76_interleaved_fields` asserts full 60-byte
  consumption.

### NIF — backlight gate `f32::MAX` (`b20e4863`)
- `rim >= f32::MAX && rim.is_finite()` correctly implements the nif.xml
  `#FLT_MAX#` sentinel (`Rimlight Power >= FLT_MAX && < FLT_INF`). Since
  `f32::MAX` is finite and `+INF` is not, the compound predicate is exactly
  `rim == f32::MAX`. Tightening from `3.0e38` removes a real over-read window
  `[3.0e38, f32::MAX)` where nif.xml says the 4-byte field is absent. Correct.

### NIF — header string-alloc bound (`b20e4863`)
- `check_header_alloc` guards `read_sized_string` (the only **u32**-length
  header string reader — call sites `header.rs:192`, `:266`) against both the
  256 MB `MAX_SINGLE_ALLOC_BYTES` cap and the bytes-remaining-in-cursor, so a
  corrupt length errors before `vec![0u8; len]`. The sibling `read_short_string`
  reads a **u8** length (max 255 B, `header.rs:432`) — no OOM surface, correctly
  left unguarded. No unguarded large-allocation reader remains.

### NIF — dead `NifVariant` predicate deletion (`8b50e238`)
- Grep across `crates/` + `byroredux/` confirms the 7 removed predicates
  (`has_material_crc`, `has_properties_list`, `avobject_flags_u32`,
  `has_shader_alpha_refs`, `has_effects_list`, `uses_bs_tri_shape`,
  `has_culling_mode`) have **zero** remaining references outside comments/docs.
  Every parse site queries raw `stream.bsver()` per the #160/#1331/#1838/#1839
  doctrine. `has_shader_property_fo3_fields` (sole survivor) retains a live
  consumer. Test files updated to drop the deleted-helper assertions.

### ESM — `visible_when_distant` threading (`a8d65d6c`, `d4b981fa`)
- New required param on `build_static_object_from_subs`. All call sites updated:
  `support.rs:254` (MODL group), `grup_walker.rs:35`, and 3 test call sites
  (passing `false`). No contract break. SCOL/PKIN/MOVS groups also populate the
  field. The `VisibleWhenDistant` marker (`SparseSetStorage`, consistent with
  the sibling `IsLodTerrain` decl) has **no render-time consumer today by
  design** — extensively documented; even the PKIN "flag rides the parent, not
  the synthetic children" nuance has zero runtime effect while no cull reads it.

### Renderer — `DBG_VIZ_MOTION` debug view (`196169a8`)
- Value `0x20000` = `131072u` is lockstep across all four sites: the Rust const
  (`shader_constants_data.rs`), the `build.rs` writer (line 342), the
  **generated** `shader_constants.glsl` (build.rs is the single source of truth —
  `include!("src/shader_constants_data.rs")`, no hand-maintained duplicate to
  drift), and the `triangle.frag` consumer. No define collision. Diagnostic is
  gated entirely behind the debug bit — zero effect on normal rendering. Targets
  still-open **#1874** (a diagnostic, not a claimed fix).
- `triangle.frag.spv` recompiled (177840 → 178504 B, consistent with the added
  branch); the descriptor-reflection test guards against a stripped-OpName
  rebuild.

### Renderer — comment/doc-only (`42adc1e6`, `e4d574dc`)
- `acceleration/constants.rs`: comment now claims `ALLOW_COMPACTION` is
  load-bearing (live compaction pass). No code change.
- `svgf.rs`: docstring updated to name the à-trous spatial pass and the
  `indirect_history` format. Verified the doc matches code —
  `INDIRECT_HIST_FORMAT = vk::Format::B10G11R11_UFLOAT_PACK32` (`svgf.rs:114`),
  exactly what the new doc states. Doc-rot correctly closed.

---

## 4. Missing tests

Both are coverage gaps only — the code under them is correct.

| # | Changed path | Gap | Severity |
|---|--------------|-----|----------|
| MT-01 | `crates/nif/src/blocks/shader.rs` (backlight gate, `b20e4863`) | The tightening from `3.0e38` to `f32::MAX` only changes behavior for a rimlight in the window `[3.0e38, f32::MAX)` (now the false/no-read branch). Existing `shader_tests.rs` cases all use exactly `f32::MAX` (still the true branch) and would pass under *either* threshold, so no test pins the tightened bound. Add a case with `rim = 3.0e38` (finite, below `FLT_MAX`) asserting `backlight_power == 0.0` and that the stream does **not** advance the extra 4 bytes. | LOW |
| MT-02 | `crates/nif/src/blocks/particle.rs` — `parse_particles_data` NiPSysData FO76 `Unknown Vector` (Vector3, 12 B, `b20e4863`) | The rotation-modifier and simple-color-modifier FO76 sites got dedicated regression tests, but the third site (the 12-byte NiPSysData skip at `particle.rs:~1277`) has none — the existing `parse_particles_data` tests are Oblivion / pre-10.4.0.1 / BS202, never FO76 (bsver 155). Add a `#BS_F76#` NiPSysData case asserting `consumed == block_size`. | LOW |

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-07-06.md
```

(Publishing is optional here — there are no severity findings. The two
missing-test items are worth filing as `low` / `tech-debt` if you want them
tracked, but neither reflects a live defect.)
