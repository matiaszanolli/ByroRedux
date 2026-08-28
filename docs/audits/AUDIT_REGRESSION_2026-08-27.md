# Regression Verification Audit — 2026-08-27

**Scope**: Full comprehensive run (`--preset comprehensive` suite member), no
`--focus` filter, solo execution — no sub-agent fan-out, per the
nested-agent-relay constraint recorded in `_audit-common.md`.

**Tracker access**: `gh` **worked** for this run, contrary to the suite brief's
warning (the `plugin:github:github` MCP server did fail to connect, but the `gh`
CLI itself authenticated and served `issue list` / `issue view` normally). Every
issue state quoted below is **live tracker state read this session**, not cached
or inferred. Cached JSON snapshots were written to `/tmp/audit/` as working
files only.

**Method**. The repo carries >2,000 closed `bug` issues, so this is a targeted
sweep, not an exhaustive one. This run deliberately weighted the sample toward
the class the suite flagged as high-value — **a CLOSED issue whose fix is absent
or only partially applied** — because that class is invisible to both the CI
traceability gate and to a naive `git log --grep` check:

1. Enumerated every issue closed since 2026-08-18 (**400**) and mapped each to a
   commit on `main` carrying an actual *closing keyword* (`Fix|Closes|Resolves
   #N`), not a bare `#N` mention. **123 of 400 (31%) have no such commit.**
2. Took the 8 **`high`/`critical`-labelled** members of that uncited set and
   verified each one's fix against the live tree, symbol by symbol.
3. Ran the skill's unconditional **Step 4** fragile-area checks.
4. Re-checked the three findings of the prior report
   (`AUDIT_REGRESSION_2026-08-24.md`) and marked each closed or still-open.

## Summary Table

| Issue | Title (abbrev.) | Status | Fix Present | Guard |
|-------|-----------------|--------|-------------|-------|
| **#3237** | SAFE-D2: GRUP-tree recursion has no depth bound | **FAIL (partial)** | **Partly — 6 of 14 sites** | `deeply_nested_grup_is_skipped_at_shared_limit` — passes, but covers only `extract_records` |
| **#3218** | REG-PROC-01: fix→issue citation link is broken | **FAIL (partial)** | Tool added, gate still cannot fire | none (advisory only) |
| **#3166** | SAVE-D1: completeness guard's `SCAN_ROOTS` covers one subdir | **PARTIAL** | Yes (1 → 6 roots) | `registry_completeness_tests.rs` — passes, 2 crates still unscanned |
| #3238 | SAFE-D9: Ball/Capsule/Cylinder have no ceiling clamp | PASS | Yes — `clamp_shape_extent` on all three arms | 3 tests (`…ball…`/`…capsule…`/`…cylinder…`) at `convert.rs:547-569` |
| #3234 | NIFAL-D8: `fill_from_bgsm` binds smoothness into specular | PASS | Yes — `smooth_spec` + `external_specular` now separate | `refr_texture_overlay_tests.rs` fixture |
| #3233 | NIFAL-D7: morph index space desyncs | PASS | Yes — `ImportedMorphTarget::original_index` | struct doc pins the invariant |
| #3222 | OBL-D5: Oblivion `WATR.TNAM` bound as a normal map | PASS | Yes — Oblivion arm writes `diffuse_texture_path` | `parse_watr` Oblivion lava/TNAM tests |
| #3213 | SKY-D6: `nMix = nA` discards layer B | PASS | Yes — the trailing statement is gone | — |
| #3099 | FO3-D4: `PLAYER_BASE_FORM_ID = 0x14` matches no NPC_ | PASS | Yes — `player_npc_form_id(game)` + loud `log::error!` | — |
| #3068 | SKY-D2: slot 2 bound as glow map unconditionally | PASS | Yes — `glow_map` / `soft_lighting` / `rim_lighting` gating | — |
| #3259 | SAFE-BUILD: `--workspace` fails to build | PASS | Yes | `cargo check -p byroredux-scripting --examples` clean |
| #3284 | FNV WaterKind vocabulary partially repaired | PASS | Yes — `creek` added; `spill`/`potomac`/`fountain` **deliberately** excluded with per-record justification | `fnv_creek_records_classify_as_river_on_both_producers` |
| #3119 | PHYS-D4: water death sites skip reconcile | PASS | Yes — `queue_dead_actor_reconciliation` at both sites | — |
| #3186 | NIFAL-D8: `texture_slot_layout` set on 1 of 4 branches | PASS | Yes — seeded at the shared boundary (`walker.rs:123`) | — |
| #3209 | `WATERLINE_HYSTERESIS` declared twice | PASS | Yes — single declaration, imported | — |
| #3210 | `records/tests.rs` is a binary file to grep | PASS | Yes | `scripts/check-text-source-integrity.sh` — clean |
| #3116 | sensors excluded only by `cast_ray` | PASS | Yes — `solid_probe_filter()` | 7 sensor-exclusion tests |
| #3373 | `sanitize_finite` misses BGEM glass-optics fields | PASS | Yes — all 4 fields, cited in place | `sanitize_finite_*` siblings |
| #3372 | chunked rebuild publishes compacted offsets early | PASS | Yes — `cd1aa9e9` "publish compacted geometry offsets only when the compacted buffer binds" | (fix commit confirmed; body not re-derived) |
| #1816 / #3287 | `translate_pex` panic-catch had no guard | PASS | Yes | `fbd6286e` added the reachable-panic guard — prior audit's REG-02 now **closed** |
| #2923 | hot-path `FxHash` conversion | PARTIAL | Yes, except one field | **Existing: #3045 (OPEN)** — see Verification Notes |
| #1857 | `context/draw.rs` monolith split | PARTIAL | Split intact; size regrown further | prior REG-2026-08-24-01, **still open** |

### Step 4 — Unconditional fragile-area checks

| Contract | Status | Evidence |
|---|---|---|
| Single material boundary (`translate_material` / `translate_texture_only_material`) | PASS | Only production `Material {` sites are `byroredux/src/material_translate.rs:440,672` plus the self-contained `--cornell` harness (no `ImportedMesh` input) |
| `Material::metalness`/`roughness` stay plain `f32` | PASS | `crates/core/src/ecs/components/material.rs:342-345` — "fully resolved, no Option" doc intact |
| Typed particle emitters (`NiPSysEmitter`/`Ctlr`/`CtlrData`/`GrowFadeModifier`) | PASS | Typed dispatch arms at `crates/nif/src/blocks/mod.rs:1043,1067,1135`; `extract_emitter_params`/`extract_emitter_rate` present; consumer `apply_emitter_params` at `byroredux/src/systems/particle.rs:29` |
| `BhkMultiSphereShape` + `BhkConvexListShape` → `CollisionShape` | PASS | `crates/nif/src/import/collision/shape.rs:110,235` |
| `resRadiance[NUM_RESERVOIRS]` stays retired | PASS | Only retrospective comments (`lighting.glsl:85`, `triangle.frag:2744`); `shadowableLightRadiance` is the live path |
| `pbr.glsl` Disney/Burley lobe + GLSL-PathTracer MIT attribution | PASS | `crates/renderer/shaders/include/pbr.glsl:10,33,142-143,155,174` |
| `GpuInstance` = 160 B, `GpuCamera` = 352 B pinned | PASS | `cargo test -p byroredux-renderer gpu_` — **41/41 pass** |
| Parser recursion caps intact (`MAX_NIF_NODE_DEPTH` 128, `MAX_COLLISION_SHAPE_DEPTH` 64, `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH` 256, `MAX_REBUILD_DEPTH` 1024) | PASS | all present and enforced; **the ESM sibling is the exception — see REG-2026-08-27-01** |

## Findings

### REG-2026-08-27-01: #3237's GRUP depth cap reaches 6 of 14 recursion sites — 8 self-recursive ESM walkers are still unbounded
- **Severity**: HIGH
- **Dimension**: Regression / ESM parser safety
- **Location**: `crates/plugin/src/esm/cell/support.rs:351,384,442,564,633,698,759` · `crates/plugin/src/esm/cell/walkers.rs:665` · guard at `crates/plugin/src/esm/reader.rs:768-786`
- **Status**: **Regression of #3237** (CLOSED 2026-08-26, `high`, `bug`)
- **Description**: #3237's premise was *"Every GRUP-tree walker in the ESM/ESP
  parser recurses into nested groups unconditionally — there is no depth
  counter."* The fix added `MAX_GRUP_NESTING_DEPTH = 64`
  (`reader.rs:32`) and a centralising helper, `bounded_group_content_end`
  (`reader.rs:768`), whose own doc comment states the intent — *"Centralising
  the guard keeps every GRUP walker on the same boundary and makes future
  recursive walkers harder to add without noticing the safety contract."*

  It was wired into **6** call sites: `grup_walker.rs:40,96,164,315`,
  `walkers.rs:132`, `wrld.rs:260` — exactly the four walkers the issue body
  *enumerated by name* (`extract_records` / `…_with_modl` /
  `extract_dial_with_info` / `extract_quest_dialogue_scene_tree_inner`) plus
  `parse_cell_group` and `parse_wrld_children`, which were correctly refactored
  into `_inner(…, depth: u32)` forms.

  **Eight further self-recursive walkers were not touched.** Each still calls the
  unguarded `group_content_end` and recurses into itself with no depth
  parameter threaded anywhere in its signature:

  | Walker | Definition | Unguarded recursion | Reached from |
  |---|---|---|---|
  | `parse_refr_group` | `walkers.rs:653` | `:665-668` | `parse_cell_group_inner` (types 6/8/9), `parse_wrld_children_inner` |
  | `parse_modl_group` | `support.rs:342` | `:351-352` | `dispatch_world_placement.rs:27` |
  | `parse_ltex_group` | `support.rs:375` | `:384-385` | `records/mod.rs:292` (`b"LTEX"`) |
  | `parse_txst_group` | `support.rs:432` | `:442-443` | `records/mod.rs:294` (`b"TXST"`) |
  | `parse_scol_group` | `support.rs:555` | `:564-565` | `records/mod.rs:303` (`b"SCOL"`) |
  | `parse_pkin_group` | `support.rs:624` | `:633-634` | `records/mod.rs:319` (`b"PKIN"`) |
  | `parse_movs_group` | `support.rs:689` | `:698-699` | `records/mod.rs:335` (`b"MOVS"`) |
  | `parse_mswp_group` | `support.rs:751` | `:759-760` | `records/mod.rs:351` (`b"MSWP"`) |

  All eight sit on the ordinary top-level GRUP dispatch path that runs on every
  `.esm`/`.esp` the engine loads, including third-party mod content — the same
  reachability argument #3237 itself made.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/cell/walkers.rs:663-669 — verbatim
  while reader.position() < end && reader.remaining() > 0 {
      if reader.is_group() {
          // Nested groups within cell children — recurse.
          let sub = reader.read_group_header()?;
          let sub_end = reader.group_content_end(&sub);
          parse_refr_group(reader, sub_end, refs, landscape, navmeshes, deleted)?;
          continue;
      }
  ```
  ```rust
  // crates/plugin/src/esm/cell/support.rs:349-354 — the shape repeated 7×
  if reader.is_group() {
      let sub = reader.read_group_header()?;
      let sub_end = reader.group_content_end(&sub);
      parse_modl_group(reader, sub_end, statics)?;
      continue;
  }
  ```
  Contrast the guarded form the same fix installed one file over:
  ```rust
  // crates/plugin/src/esm/cell/walkers.rs:130-135
  let sub_group = reader.read_group_header()?;
  let Some(sub_end) =
      reader.bounded_group_content_end(&sub_group, depth, "parse_cell_group")
  else {
      continue;
  };
  ```
  The single guard test, `deeply_nested_grup_is_skipped_at_shared_limit`
  (`crates/plugin/src/esm/records/grup_walker.rs:394-419`), builds
  `MAX_GRUP_NESTING_DEPTH + 128` nested `GRUP`s and feeds them to
  `extract_records` only. Substituting any of the eight walkers above into that
  same fixture reproduces the pre-#3237 unbounded descent — the fixture is
  already written, it simply never points at them.
- **Impact**: The stack-overflow-on-crafted-plugin vector #3237 was closed for
  is still live through eight independent entry points. A `GRUP` header is
  20–24 bytes, so a few hundred KB of nested minimal groups drives the recursion
  tens of thousands of levels deep and aborts the process — an uncatchable
  crash, not a `Result`-typed parse failure. `parse_refr_group` is the worst of
  the eight: it carries six `&mut` parameters and a large local set, so its
  frame is among the biggest in the parser, and it is reachable from *both*
  the interior-cell and worldspace descents.
- **Related**: #3237 (the partially-applied fix), #3279 (OPEN — same defect
  class in `Effect::Conditional`'s `lower_statements`), #1385 (`MAX_COLLISION_
  SHAPE_DEPTH`, the reference model), REG-2026-08-27-02 (the traceability gap
  that let this close silently — #3237's fix landed inside mega-commit
  `06f86742` under the line *"refactor(plugin): implement bounded group content
  parsing for ESM readers"*, with no closing keyword and no per-site accounting)
- **Suggested Fix**: Convert all eight to the `_inner(…, depth: u32)` +
  `bounded_group_content_end` form the fix already established, then extend
  `deeply_nested_grup_is_skipped_at_shared_limit` into a table-driven test that
  drives the *same* nested fixture through every recursive walker, so the next
  walker added without a depth parameter fails CI. Consider making
  `group_content_end` `pub(super)`-restricted or `#[deprecated]`-annotated so
  the unguarded helper is hard to reach from a new walker at all.

### REG-2026-08-27-02: #3218's traceability fix shipped an advisory tool, and the citation gap it measured is unchanged
- **Severity**: MEDIUM
- **Dimension**: Regression / audit infrastructure
- **Location**: `.github/workflows/ci.yml:15-25` · `scripts/check-issue-traceability.sh:34-52` · `.claude/commands/session-close/SKILL.md:80-88`
- **Status**: **Regression of #3218** (CLOSED 2026-08-26)
- **Description**: #3218 diagnosed the mechanism precisely — the CI gate is
  `if: github.event_name == 'pull_request'`, and *"this repo's history is
  overwhelmingly direct commits to main, so for the dominant workflow it never
  fires."* The fix added a `--window` mode to
  `scripts/check-issue-traceability.sh` and a call to it in the `session-close`
  ritual. **The gate's trigger condition was not changed**, and the `--window`
  mode is a report, not an enforcement — nothing fails, nothing blocks, and
  nothing back-fills the citation.

  The measured gap has therefore not moved. #3218 was filed against *43 of 134
  (32%)* uncited in the 2026-08-16..20 window. Measured this session over the
  2026-08-18..28 window: **123 of 400 (31%)**. The rate on 2026-08-26 — the day
  #3218 itself closed — was **36 of 76 (47%)**.
- **Evidence**:
  ```
  # .github/workflows/ci.yml:15-17 — unchanged trigger
  name: Issue/commit traceability
  if: github.event_name == 'pull_request'
  ```
  ```
  per-day uncited (closing-keyword commit on main, live gh state):
    2026-08-18: 24/58 (41%)     2026-08-24:  0/3  ( 0%)
    2026-08-19:  2/37 ( 5%)     2026-08-25:  6/19 (32%)
    2026-08-20:  2/11 (18%)     2026-08-26: 36/76 (47%)  <- #3218 closed
    2026-08-21:  2/61 ( 3%)     2026-08-27:  5/47 (11%)
    2026-08-22: 44/64 (69%)     2026-08-28:  2/17 (12%)
    2026-08-23:  0/7  ( 0%)
  ```
  ```
  # commits on main since 2026-08-20 touching *.rs
  246 total — 122 (50%) carry no closing keyword
  ```
  The reverse direction — *commit → issue* — is not checked at all: the script's
  `closing_issue_numbers` reads a PR body, and `--window` iterates closed
  issues. A commit that fixes something with no issue attached is invisible to
  both modes.
- **Impact**: This is the mechanism that produced REG-2026-08-27-01. #3237's
  partial fix was buried in a 12-line mega-commit body under a `refactor(...)`
  heading with no closing keyword; the issue was closed by hand; no gate
  compared the fix's reach to the issue's stated scope. As #3218's own script
  comment says, the degradation is self-concealing: *"a regression audit that
  cannot find fixes gets quieter, not louder."* At a 31% uncited rate,
  `/audit-regression`'s Step 2 (`git log --grep="#<N>"`) is a coin flip, and
  every future sweep pays the cost of re-deriving fix presence by hand.
- **Related**: #3218, REG-2026-08-27-01
- **Suggested Fix**: Change the gate to run on `push` to `main` over the pushed
  range (`github.event.before..github.event.after`) rather than only on
  `pull_request`, so the dominant workflow is actually covered. Separately, add
  the missing direction — flag pushed commits that touch `*.rs` and cite no
  issue at all — as a warning-level annotation, so the fix→issue link is
  recorded while the context is fresh rather than reconstructed by an auditor
  weeks later.

### REG-2026-08-27-03: #3166's `SCAN_ROOTS` expansion still leaves two crates unscanned, and three production `Resource` impls invisible
- **Severity**: LOW
- **Dimension**: Regression / save-load completeness guard
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:362-369`
- **Status**: **Regression of #3166** (CLOSED, `medium`) — partial fix
- **Description**: #3166's title is *"the completeness guard's `SCAN_ROOTS`
  covers one subdirectory of `crates/core`"*. The fix widened it from 1 root to
  6 (`crates/core`, `crates/scripting`, `crates/physics`, `crates/audio`,
  `crates/plugin`, `byroredux`), which is a real improvement. But the guard's
  contract is *completeness* — every live ECS `Component`/`Resource` is either
  registered for save or named in the exclusion ledger — and two workspace
  crates that declare production `Resource` impls are still outside the scan,
  with no comment recording the exclusion as deliberate.
- **Evidence**:
  ```rust
  // byroredux/src/save_io/registry_completeness_tests.rs:362-369 — verbatim
  const SCAN_ROOTS: &[&str] = &[
      "../crates/core/src",
      "../crates/scripting/src",
      "../crates/physics/src",
      "../crates/audio/src",
      "../crates/plugin/src",
      "../byroredux/src",
  ];
  ```
  Production `impl Resource for` sites outside those roots:
  ```
  crates/renderer/src/vulkan/allocator.rs:49  impl Resource for AllocatorResource {}
  crates/renderer/src/vulkan/allocator.rs:70  impl Resource for GpuMemoryBudget {}
  crates/save/src/registry.rs:18              impl Resource for SaveRegistry {}
  ```
  (`crates/nif`, `crates/ui`, `crates/bsa`, `crates/papyrus`, `crates/pex`,
  `crates/debug-server`, `crates/platform`, `crates/sfmaterial`, `crates/bgsm`
  declare none, so the blind spot is exactly these two crates today.)
- **Impact**: No live bug — all three are engine machinery (a GPU allocator
  handle, a VRAM budget probe, and the save registry itself) that must never be
  serialised, and none carries gameplay state. The gap is in the guard's reach:
  a future saveable `Resource` declared in `crates/renderer` or `crates/save`
  would be silently absent from both the registry and the exclusion ledger,
  which is precisely the failure mode #3166 exists to prevent.
- **Related**: #3166, #3167 (sibling — *"the rewritten serde guard's file
  discovery has three residual holes"*), #2536
- **Suggested Fix**: Either add the two roots and enumerate the three types in
  the existing exclusion table with their one-line justification, or add a
  comment above `SCAN_ROOTS` stating which crates are deliberately out of scope
  and why — the current silence is indistinguishable from an oversight.

### REG-2026-08-27-04: `context/draw.rs` regrowth past the #1857 baseline continues
- **Severity**: LOW
- **Dimension**: Tech debt / Renderer
- **Location**: `crates/renderer/src/vulkan/context/draw.rs`
- **Status**: **Still open** — carried forward from REG-2026-08-24-01, unchanged in kind
- **Description**: The prior report measured `draw.rs` at 4,909 LOC (pre-#1857
  baseline: 4,808) with `draw_frame` at 2,493 LOC. Four days later:
  **4,947 LOC**, `draw_frame` **2,509 LOC**. The three files #1857 split out
  (`geometry_pass.rs` / `post_passes.rs` / `skinned_blas_refit.rs`) remain
  intact — this is not a reverted fix — but nothing enforces the boundary, so
  new pass code keeps landing in the monolith.
- **Evidence**:
  ```
  wc -l crates/renderer/src/vulkan/context/draw.rs   # 4947  (was 4909 on 08-24)
  draw_frame body                                    # 2509  (was 2493 on 08-24)
  ```
- **Impact**: None functional. Informational for `/audit-tech-debt`.
- **Suggested Fix**: Unchanged from the prior report — a soft LOC guard test
  mirroring the GPU-struct size pins would catch regrowth before it compounds.

## Prior-Report Reconciliation (`AUDIT_REGRESSION_2026-08-24.md`)

| Prior finding | Disposition |
|---|---|
| REG-2026-08-24-01 — `draw.rs` regrown past #1857 baseline | **Still open**, marginally worse — re-filed above as REG-2026-08-27-04 rather than duplicated |
| REG-2026-08-24-02 — `translate_pex` panic-catch has no guard | **CLOSED**. Published as #3287, fixed by `fbd6286e` ("make the translate_pex panic guard reachable from a test"); the reachable-panic seam and its guard are present at `crates/scripting/src/translate/mod.rs:121-170` |
| REG-2026-08-24-03 — two `SAFETY:`-labelled unsafe blocks have no guard | **Still open by design**, informational only; no automated guard is practical and none was expected |

## Verification Notes

- **#3045 (OPEN) confirmed live, not a new finding.** The `#2923` hot-path
  `FxHash` contract that `_audit-common.md` describes as holding *"end-to-end"*
  still has one std-hasher hole: `skin_dispatch_seen_scratch:
  std::collections::HashSet<EntityId>` (`crates/renderer/src/vulkan/context/mod.rs:1325`),
  probed once per draw command per frame at
  `skinned_blas_refit.rs:102-112` (`seen.insert(dc.entity_id)`). The #2923 guard
  (`pose_dirty_accessor_does_not_pin_siphash_across_the_crate_boundary`,
  `crates/core/src/ecs/resources/skin_slot_pool.rs:947-963`) scans only
  `skin_slot_pool.rs`'s own source, so it structurally cannot see a renderer-side
  field. Filed and open since 2026-08-16 as **#3045**; recorded here as a
  dedup-verified existing issue, not re-reported.
- **The 7 issues the suite brief flagged as "fixed in code but still OPEN"**
  (#3244, #3270, #3191, #3149, #3151, #3155, #3156) were re-checked against live
  `gh` state: all 7 are indeed **OPEN**. Determining whether each is genuinely
  already fixed is a per-subsystem question outside this audit's charter; they
  are noted here so the next sweep does not treat them as unexamined.
- **#3284's "partial repair" is deliberate, not a gap.** `spill`, `potomac` and
  `fountain` are absent from the WaterKind vocabulary *on the evidence* —
  `byroredux/src/material_translate.rs:205-221` records the per-record survey
  (`ToxicSpillPuddle` is a puddle, `TenPenWaterFountain` / `VStripULFountain`
  are not rivers) and the guard
  `fnv_creek_records_classify_as_river_on_both_producers` pins the token that
  *was* added. PASS.
- **No FAIL was found where a fix was wholly absent.** Every one of the 8
  uncited `high`/`critical` closures sampled had its fix present in the live
  tree; the single defect was #3237's *incomplete* application. The
  fix-absent-entirely case did not occur in this sample.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-27.md
```
