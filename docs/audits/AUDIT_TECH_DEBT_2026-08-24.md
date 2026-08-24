# Tech-Debt Audit — 2026-08-24

**Depth**: deep · **Dimensions**: all 9 · **Sweep**: comprehensive, standalone
(single-agent, no sub-agent fan-out) · **Delta**: 108 commits since
`AUDIT_TECH_DEBT_2026-08-20.md`, continuing the WATAL convergence pass plus a
fresh, unaudited burst of same-day (2026-08-24) work: the #3231 GPU
morph-target pipeline, #2221's animated-sink → `GpuMaterial` wiring, quest
fragment dispatch / global-variable / trigger-gating scripting features, the
actor-value key-space unification (#2987 follow-through), save/load
notifications, and a scheduler `WindField` access change touching
`crates/core/src/ecs/lock_tracker.rs`.

## Scope

Whole workspace (24 crates + `byroredux/`). No cargo was run — `cargo test
--workspace` is documented as broken by an unrelated E0004 in
`crates/scripting/examples/fragment_coverage.rs` (owned by `/audit-scripting`)
— all analysis is static: grep, `git log -S`/`git blame`, `git show --stat`,
and the `prod_loc` helper from the SKILL's Phase 1. Executed entirely by one
agent, directly, per the dispatch's explicit no-sub-agent constraint.

Weighted toward the newest, least-reviewed code per the SKILL's own callout
(`crates/pex`, `crates/save`, `crates/hkx`, `crates/scripting`) and toward
today's commits specifically, since nothing has audited them yet.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 2 |
| **Total** | **3** |

Per-dimension yield:

| Dim | Area | Findings |
|---|---|---|
| 1 | File / Function / Module Complexity | **1** (LOW) |
| 2 | Logic Duplication | **0 — CLEAN** (spot-checked; nothing new) |
| 3 | Stale Documentation & Comments | **2** (1 MEDIUM, 1 LOW) — the MEDIUM also carries a Dim 7 (magic-number/doc) angle, deduped here |
| 4 | Audit-Finding Rot | **0 — CLEAN** (validate gate passes; advisories triaged, all either benign or already tracked) |
| 5 | Stale Markers (TODO/FIXME/HACK/XXX) | **0 — CLEAN** |
| 6 | Stub & Placeholder Implementations | **0 — CLEAN** |
| 7 | Magic Numbers & Hardcoded Constants | **0 — CLEAN** (folded into the Dim 3 finding above; no standalone hit) |
| 8 | Dead Code & Backwards-Compat Cruft | **0 — CLEAN** (count grew 60→69 but every new site is the known `#[cfg_attr(not(debug_assertions))]`/`quest.rs` pattern) |
| 9 | Test Hygiene | **0 — CLEAN** (`#[ignore]` grew 154→171, all data/GPU-gated; recipe-scan bug reconfirmed, already tracked) |

**Headline**: `GpuInstance` grew **128 → 160 bytes** on `5f4dea46`/`d0322785`
(2026-08-23, #3231, GPU morph-target blending) and `GpuMaterial` grew
**348 → 364 bytes** on `7fbc5baf` (2026-08-23, #2221, animated-sink fields).
This is the *exact same failure mode* as the still-open `GpuCamera` 336→352
finding (#3201, filed 2026-08-20, unresolved four days later) — new struct
growth outpaces the reference docs. Five sites across `docs/engine/*.md` plus
one in-code doc comment (`constants.rs`) plus one audit-infrastructure file
(`audit-safety/SKILL.md`) still cite the old sizes or old test names, one of
which — `constants.rs:176` citing `gpu_material_size_is_348_bytes` — is
itself the reason the validate gate's symbol advisory misses it: the stale
string exists verbatim in tracked source (as *another* stale doc comment),
so the "symbol not found anywhere" heuristic can't see a doc comment
agreeing with a doc comment. `.claude/commands/audit-renderer/SKILL.md` and
`audit-performance/SKILL.md` were already fixed same-day (`048a8bd8`) —
`docs/engine/*.md`, the tier `_audit-common.md` designates as the
*authoritative* reference, was not.

The rest of the sweep is quiet. `crates/scripting/src/trigger.rs` (602 new
lines today: actor-gated quest triggers, tethered-horse detection) is clean —
one named constant, no duplication, functions under the 200-LOC trigger. The
squashed `4e1afcbe` commit's headline claim ("unify actor value key space")
independently verified as fixing the exact drift `REG-2026-08-20-D6-01`
(#3216) flagged four days ago. `crates/core/src/ecs/lock_tracker.rs`'s one new
`#[cfg_attr(not(debug_assertions), allow(dead_code))]` is the documented
release/debug storage-parity pattern, not new debt. `WATERLINE_HYSTERESIS`
duplication (#3209) is fixed and consolidated. The `context/mod.rs`
1025-LOC-constructor finding (#1749) is fixed — `mod.rs` shrank from 4059 to
2306 production LOC via the `init.rs`/`teardown.rs` extraction — but
`draw_frame` in the sibling `draw.rs` re-grew to 2498 LOC in the interim,
the same regrowth pattern #2255 (closed 2026-07-25) previously caught once
already.

### Premises investigated and not sustained

- *"The 18 validate-gate advisory symbols are all fresh drift worth
  filing."* Triaged individually: `IsCollisionOnly` is Existing #2638 (dead
  marker removed under #1570, docs never caught up — still open, not
  re-filed). `gl_InstanceID`/`LateExclusive`/`ParallelUpdate`/`SystemAccess`
  are intentional *"NOT X" / "there is no X"* negative references —
  stylistically should be italicised rather than backticked per the
  path-reference convention, but this is the same cosmetic gap the mechanism
  finding #3052/TD4-2026-08-20-01 already covers, not a fresh instance
  worth re-filing. `WhiterunDragonsreach`/`query_N_mut`/`SurfaceClass`/
  `PlacementLodProvider` are false positives of treating a cell name, a
  generic pattern name, a forward-looking invariant, and descriptive prose
  as literal symbols. `declines_on_control_flow` (audit-scripting/SKILL.md)
  *is* live drift — renamed to `declines_unmodeled_conditional_guard` by
  today's `cee35507` — and is filed below as new.
- *"The `#[ignore]` count is 503, a 3.3× regression from 154."* Reproduced
  the SKILL's own bare recipe (`grep -RIn '#\[ignore\]' .`) and it does
  read 503 today — but 313 of those hits are inside `docs/`/`.claude/`
  markdown (test names quoted in prose), not `.rs` files. Restricting to
  `--include='*.rs'` gives 171, a real but modest 154→171 growth, entirely
  attributable to the water-test buildout. This is **Existing #2262**
  (*"Tech-debt skill's own Phase-1 recipe scans the whole repo textually,
  producing a false ~2.4× regression signal"*), reconfirmed still live at a
  slightly worse ratio (2.9×) — not re-filed, but worth flagging since it
  nearly produced a false HIGH-looking headline in this very report.
- *"`crates/scripting`'s ~4500-line same-day delta (quest fragments,
  triggers, globals) hides fresh debt."* Spot-checked the largest single
  file change (`trigger.rs`, +602 lines) and the renamed-test drift above;
  found the one real item (the stale citation) and nothing else — no new
  magic numbers, no duplicated scaffolding, functions stay under the
  200-LOC trigger. Not a full per-file sweep of the whole delta (out of
  budget for a single-agent run), but the sample carries no further debt.

---

## Baseline Snapshot (for the next audit's diff)

```
TODO/FIXME/HACK/XXX:        20   (0 real — all protocol / upstream-ref / prose; unchanged since 08-20)
allow(dead_code):            69  (was 60; +9, dominated by quest.rs's ALIAS_FLAG_* cluster +1 and
                                   the documented cfg_attr(not(debug_assertions)) release-build pattern)
unimplemented!/todo!():       0  (unchanged)
#[ignore] tests (*.rs only): 171  (was 154; the bare "." recipe over the whole tree reads 503 —
                                   313 are docs/markdown false hits, see Existing #2262)
files >2000 production LOC:   4  (unchanged membership — see below; #1749 fixed, #2977 fixed,
                                   but both member sets stayed at 4 by different files re-crossing)
files >2000 total LOC:       19  (was 16 in the 08-20 snapshot's methodology; membership churned —
                                   see the secondary-bucket table below)
```

Dim-1 **production** bucket (the dimension's actual subject) — re-verified
with `prod_loc`:

| Prod LOC | Total LOC | File | Issue | Delta vs 08-20 |
|---|---|---|---|---|
| 3580 | 4909 | `crates/renderer/src/vulkan/context/draw.rs` | none open — see TD1-2026-08-24-01 | +84 prod, +179 total |
| 2859 | 3745 | `crates/renderer/src/vulkan/volumetrics.rs` | #2256 (OPEN) | +85 prod, +289 total |
| 2306 | 2770 | `crates/renderer/src/vulkan/context/mod.rs` | #1749 **CLOSED** (`6fad32ac`, `new()`/`Drop` extracted to `init.rs`/`teardown.rs`) | **−1753 prod**, −1718 total |
| 2013 | 2021 | `crates/renderer/src/texture_registry.rs` | #2977 **CLOSED** (recorded, no split warranted — majority genuine production) | unchanged |

Secondary (test-heavy / below-threshold) bucket sampled with `prod_loc` —
none crosses into the primary bucket:

| File | Total | Prod | Note |
|---|---|---|---|
| `crates/plugin/tests/parse_real_esm.rs` | 2516 | 0 | pure test |
| `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` | 2472 | 0 | pure test |
| `byroredux/src/env_translate.rs` | 3241 | 1351 | majority test (water/exterior test buildout) |
| `crates/physics/src/world.rs` | 2608 | 1163 | majority test |
| `crates/plugin/src/esm/records/misc/water.rs` | 2454 | 1517 | grew from 2081/1418 (08-20); still below threshold |
| `crates/scripting/src/fragment/tests.rs` | 2309 | 0 | pure test, new to the list (today's quest-fragment work) |
| `crates/plugin/src/esm/records/misc/world.rs` | 2274 | 1234 | majority test |
| `crates/renderer/src/vulkan/svgf.rs` | 2266 | 1715 | stable |
| `crates/renderer/src/vulkan/material.rs` | 2223 | 1368 | stable, matches #2257's "mostly test growth" finding |
| `crates/scripting/src/translate/effects.rs` | 2140 | 1255 | grew with today's quest-effect work; still below threshold |
| `byroredux/src/cornell.rs` | 2114 | 1637 | unchanged |
| `byroredux/src/systems/animation.rs` | 2100 | 1022 | new to the list |
| `crates/renderer/src/mesh.rs` | 2053 | 1525 | new to the list |
| `crates/nif/src/import/collision/shape.rs` | 2044 | 730 | majority test |
| `crates/ui/src/avm2_host.rs` | 2008 | 1159 | new to the list |

`prod_loc` behaved correctly on every file checked. No disagreement with the
SKILL's own filed evidence this cycle.

---

## Top Quick Wins (trivial, ≤30 min each)

1. **TD3-2026-08-24-01** — fix `GpuInstance` 128→160 and `GpuMaterial`
   348→364 across the five `docs/engine/*.md` sites, `constants.rs:176`, and
   `audit-safety/SKILL.md:220`. Same shape as the still-open #3201 GpuCamera
   fix; doing both together in one pass is strictly cheaper than two.
2. **TD3-2026-08-24-02** — retarget `audit-scripting/SKILL.md:871`'s
   `declines_on_control_flow` citation to `declines_unmodeled_conditional_guard`
   (`crates/scripting/src/translate/effects.rs:2125`, renamed today by
   `cee35507`).
3. Re-run `.claude/commands/_audit-validate.sh` after (1) and (2) — expect the
   advisory list to drop by at least one entry (`declines_on_control_flow`);
   `gpu_material_size_is_348_bytes` will *not* disappear from the advisory
   mechanism even after the fix, since the corrected comment will legitimately
   cite the live `_364_` name — worth noting so the next sweep doesn't
   re-flag it as still-broken tooling.

## Top Medium Investments

1. **TD1-2026-08-24-01** — decompose `draw_frame` (2498 LOC, 51% of
   `draw.rs`) along its own existing phase markers (fence-wait/acquire →
   deferred-destroy → cmd-begin/TLAS-build → camera+light+TAA/DOF assembly →
   skin/bone GPU upload+dispatch → cluster-cull dispatch → instance-SSBO
   build+upload → material/terrain reupload), mirroring the
   `record_geometry_pass`/`record_post_passes` extractions the function
   already delegates to at its tail. This is the third occurrence of the same
   pattern (#2255 closed once; #1749's sibling constructor closed via the
   same axis) — worth a durable fix, not another point extraction.
2. Carry-over from 08-20, still the two widest-blast-radius OPEN items:
   #2256 (`volumetrics.rs`, now 2859 prod LOC, +85 since last measured) and
   the recurring GPU-struct-doc-drift pattern (#3201 open 4 days, this
   report's TD3-2026-08-24-01 the same failure shape one struct-growth later)
   — the structural fix for the latter, extending `_audit-validate.sh` to
   `docs/engine/*.md` (#3202/TD4-2026-08-20-03), is itself still open and
   would have caught both.

---

# Findings

## MEDIUM

### TD3-2026-08-24-01: `GpuInstance` grew 128→160 B and `GpuMaterial` grew 348→364 B yesterday; five `docs/engine/*.md` sites, one in-code doc comment, and one audit-skill still cite the old sizes

- **Severity**: MEDIUM
- **Dimension**: 3 — Stale Documentation & Comments (severity-table promotion:
  *"Stale `GpuCamera`/`GpuInstance`/`GpuMaterial` size in a doc comment
  (lockstep-drift bait)"* → MEDIUM floor)
- **Location**:
  - `docs/engine/shader-pipeline.md:247` (`GpuInstance` heading, "128 bytes")
    and its field table `:251-266` (ends at offset 120 `_reserved`, "no live
    data" — the three morph-target fields and the real padding tail at
    128-160 are absent, same truncation shape as the GpuCamera table in
    #3201)
  - `docs/engine/shader-pipeline.md:283` (`GpuMaterial` heading, "348 bytes")
  - `docs/engine/memory-budget.md:31` (`Instance SSBO … 128 B (#2219) … 33.6
    MB … 67.1 MB`) and `:34` (`Material SSBO … 348 B … 5.7 MB … 11.4 MB`)
  - `docs/engine/renderer.md:133-134`, `:528-531` (three prose sites, "128
    bytes"/"348 bytes")
  - `docs/engine/renderer.md:577` — names two dead test symbols in one line:
    `gpu_instance_is_128_bytes_std430_compatible` (NEW — this finding) and
    `gpu_camera_is_336_bytes` (Existing #3201)
  - `crates/renderer/src/vulkan/scene_buffer/constants.rs:172-176` — the
    `MAX_MATERIALS` doc comment: `"16384 × 348 B ≈ 5.7 MB … 11.4 MB total"`
    and `"pinned by \`gpu_material_size_is_348_bytes\`"`
  - `.claude/commands/audit-safety/SKILL.md:220` — `"GpuMaterial size is
    pinned at 348 B by gpu_material_size_is_348_bytes"`
- **Status**: NEW
- **Age**: `5f4dea46`/`d0322785` (2026-08-23, #3231 — GPU morph-target
  blending grew `GpuInstance`) and `7fbc5baf` (2026-08-23, #2221 — animated-
  sink fields grew `GpuMaterial`). Every stale site above predates or was not
  touched by either commit.
- **Effort**: trivial (the doc edits) + the mechanism note below is small
- **Description**: The Rust-side source of truth is correct and unusually
  well-documented — `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:65-90`
  carries a full layout-history comment ending `128 → 160 (#3231, …)`, the
  GLSL mirror in `include/bindings.glsl:22-70` is updated field-for-field
  (camelCase `morphDeltaAddress`/`morphWeightAddress`/`morphTargetCount`,
  correctly three scalar `uint`s rather than a std430-footgun `uvec3`, with
  a doc comment explaining why), and the pinned test is
  `gpu_instance_is_160_bytes_std430_compatible` /
  `gpu_material_size_is_364_bytes`
  (`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:30`,
  `crates/renderer/src/vulkan/material.rs:1421`). Two audit-infrastructure
  files were even fixed same-day: `048a8bd8` updated
  `audit-renderer/SKILL.md:115` and `audit-performance/SKILL.md:103` to the
  correct 160 B / 364 B figures with full growth histories.

  What was missed is exactly the tier `_audit-common.md`'s Key Reference Docs
  table designates as *authoritative* — `docs/engine/shader-pipeline.md` is
  named there as the source for "exact byte layouts", and every audit is told
  to "prefer them over re-deriving facts from source". A reader following
  that instruction today gets two wrong sizes and a `GpuInstance` field table
  that silently truncates the struct at 120/160 bytes — the identical failure
  shape TD3-2026-08-20-01 found four days ago for `GpuCamera`, which remains
  open (#3201) as of this report.

  The `constants.rs` and `audit-safety/SKILL.md` sites add a new wrinkle:
  `constants.rs:176`'s doc comment cites `gpu_material_size_is_348_bytes` by
  name. That string exists verbatim in the tracked source tree — inside this
  very doc comment — so the validate gate's symbol-existence check
  (`grep -qw "$sym" "$src_blob"`) finds a match and never flags it as an
  advisory. Two stale citations agreeing with each other are invisible to a
  "does this symbol appear anywhere" heuristic; the gate's fix for the
  case/negation blind spots (#3197) did not anticipate a *self-referential*
  blind spot where the stale claim is itself the only appearance of the
  string. This is a third structural gap in the same mechanism TD4-2026-08-20-01
  and TD4-2026-08-20-03 already catalogued, worth naming for whoever next
  works on `_audit-validate.sh`, though not severe enough on its own to file
  as a fourth separate mechanism finding.
- **Evidence**:
  ```
  $ grep -n "fn gpu_instance_is\|fn gpu_material_size_is" \
        crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs \
        crates/renderer/src/vulkan/material.rs
  gpu_instance_layout_tests.rs:30:fn gpu_instance_is_160_bytes_std430_compatible() {
  material.rs:1421:    fn gpu_material_size_is_364_bytes() {

  $ grep -rn "gpu_instance_is_128_bytes_std430_compatible\|gpu_material_size_is_348_bytes" crates byroredux
  crates/renderer/src/vulkan/scene_buffer/constants.rs:176:/// size is pinned by `gpu_material_size_is_348_bytes`; it was 300 B until
  docs/engine/renderer.md:577:> `gpu_instance_is_128_bytes_std430_compatible`, `gpu_camera_is_336_bytes`
  .claude/commands/audit-safety/SKILL.md:220:- **`GpuMaterial` size is pinned at 348 B** by `gpu_material_size_is_348_bytes`

  $ sed -n '65,90p' crates/renderer/src/vulkan/scene_buffer/gpu_types.rs | tail -6
  ///   - 112 → 128 (#2219, `skinned_vertex_address` + reserved padding)
  ///   - 128 → 160 (#3231, `morph_delta_address` + `morph_weight_address`
  ///     + `morph_target_count` + reserved padding)

  $ sed -n '247,266p' docs/engine/shader-pipeline.md | tail -3
  | 108 | 4 | `surface_id` | … |
  | 112 | 8 | `skinned_vertex_address` | … #2219 … |
  | 120 | 8 | `_reserved` | Padding to a 16-byte-aligned std430 stride — no live data |
  ```
  The GLSL mirror IS current (confirmed by direct read, not grep — the field
  names differ by naming convention so a plain grep for the Rust field names
  finds nothing):
  ```
  $ sed -n '61,70p' crates/renderer/shaders/include/bindings.glsl
      uint64_t morphDeltaAddress;  // offset 128, 8 bytes
      uint64_t morphWeightAddress; // offset 136, 8 bytes
      uint morphTargetCount;       // offset 144, 4 bytes
      uint _reserved2a; // offset 148, 4 bytes
      uint _reserved2b; // offset 152, 4 bytes
      uint _reserved2c; // offset 156, 4 bytes -> total 160
  };
  ```
- **Impact**: Nothing is broken at runtime — the shader/Rust pair is in
  lockstep and the discrepancy is confined to reference material. The damage
  is exactly what #3201 already documents for `GpuCamera`: a contributor or
  auditor who trusts the designated-authoritative doc gets a wrong byte count
  and, for `GpuInstance`, a field table that stops 40 bytes short of the real
  struct. `memory-budget.md`'s two affected rows also understate VRAM by a
  small but compounding amount (Instance SSBO: 67.1→83.9 MB at the documented
  formula; Material SSBO: 11.4→11.9 MB) — not itself a safety issue at
  current `MAX_INSTANCES`/`MAX_MATERIALS`, but it is the kind of drift
  `AUDIT_PERFORMANCE`/`AUDIT_SAFETY` reports have flagged before when it
  compounds across several rows.
- **Related**: #3201/TD3-2026-08-20-01 (identical failure shape, one struct
  earlier, still open); #3202/TD4-2026-08-20-03 (the structural fix —
  extending the validate gate to `docs/engine/*.md` — that would have caught
  both); #3240 (closed — the same `GpuMaterial` 348 B stale-comment pattern,
  already fixed once in `bindings.glsl` specifically, now recurring
  elsewhere after the *next* growth).
- **Suggested Fix**: Update all seven listed sites to 160 B / 364 B in one
  pass; add the three missing `GpuInstance` field-table rows
  (`morph_delta_address`, `morph_weight_address`, `morph_target_count` +
  padding) to `shader-pipeline.md`; recompute the two `memory-budget.md`
  rows; retarget `renderer.md:577`'s test-name citation to
  `gpu_instance_is_160_bytes_std430_compatible` (leave the `GpuCamera` half
  of that line for #3201's own fix, or fix both together since they're on
  the same line). Land #3202 while in the area so the *next* struct growth
  is caught mechanically instead of needing a fourth manual sweep.

---

## LOW

### TD1-2026-08-24-01: `draw_frame` re-grew to 2498 LOC — 51% of a 4909-line file — the same regrowth pattern #2255 closed once already

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1522-4020`
  (`pub fn draw_frame`)
- **Status**: NEW (the *file* is tracked in the Dim-1 primary bucket with no
  open issue after #2977 closed; #2255, closed 2026-07-25, covered an earlier
  regrowth of this same function and is not reopened by this finding — this
  is a fresh instance of the pattern, not a regression of the closed fix,
  since #2255's own specific extraction still holds)
- **Effort**: medium (well-bounded — see the split axis below, which mirrors
  work the file has already done twice for its own tail)
- **Description**: `draw_frame` is 2498 LOC in a 4909-line file — the single
  largest function in the codebase's primary Dim-1 bucket by a wide margin.
  It is not tangled: reading through it top-to-bottom shows a linear pipeline
  with clear, already-commented phase boundaries (several reference an
  explicit "Phase N" numbering: `"Bracketed (Phase 9)"` for the swapchain
  acquire), and the function already delegates its tail to two previously
  extracted helpers — `self.record_geometry_pass(...)` at `:3611` (#2258's
  sibling extraction) and `self.record_post_passes(...)` at `:3655`
  (`record_post_passes`, #2258). What remains inline is everything
  *upstream* of those two calls: frame-sync (fence wait, image acquire,
  deferred-destroy tick), command-buffer begin, TLAS build, camera/light/TAA-
  jitter/DOF/camera-cut assembly, skin/bone GPU upload + palette-build
  dispatch, cluster-cull dispatch, the instance-SSBO build + upload (the
  single largest sub-block, encompassing the draw-command → `GpuInstance`
  translation and the `TLAS.instance_custom_index` ↔ SSBO-index contract),
  and the material-table + terrain-tile reupload — none of which has been
  factored out the way the render-pass recording itself was.
- **Evidence**:
  ```
  $ awk '/^    pub fn draw_frame/{start=NR} /^    fn should_skip_skin_gpu_refresh/{print NR-start; exit}' \
        crates/renderer/src/vulkan/context/draw.rs
  2498

  $ wc -l crates/renderer/src/vulkan/context/draw.rs
  4909 crates/renderer/src/vulkan/context/draw.rs   # draw_frame is 51%

  $ grep -n "self.record_geometry_pass(\|self.record_post_passes(" crates/renderer/src/vulkan/context/draw.rs
  3611:        self.record_geometry_pass(
  3655:            self.record_post_passes(
  ```
  No open issue currently names `draw_frame`'s size directly — `gh issue list
  --search "draw_frame in:title,body" --state open` returns only #2519 (an
  unrelated correctness finding about mid-frame dispatch failure). #2255
  (closed 2026-07-25) covered an earlier version of this same regrowth; its
  own extraction still holds (verified: the function it split out has not
  been re-inlined), so this is the *pattern recurring a second time*, not a
  reopened regression.
- **Impact**: Maintenance cost only. Every new per-frame GPU resource (this
  session alone added the morph-target dispatch and the animated-sink
  material fields) has a natural landing spot inside this one function, and
  the file's own history shows that pattern compounding — `draw.rs` grew
  from 4730 to 4909 total lines in four days entirely inside this function.
  Reviewing any single change to it means paging past thousands of unrelated
  lines of frame-setup code.
- **Related**: #2255 (closed — the prior instance of this exact regrowth
  pattern); #2258/#2259 (the two extractions this finding proposes
  extending — `record_post_passes`/`build_tlas` split, cited in
  `_audit-common.md`'s note that "file-level crossings and function-level
  splits are independent signals").
- **Suggested Fix**: Extract along the function's own already-commented
  phase boundaries into private helpers on `VulkanContext`, mirroring the
  `record_geometry_pass`/`record_post_passes` pattern already in use:
  `sync_and_acquire_frame` (fence wait → deferred-destroy → image acquire),
  `begin_frame_recording` (cmd reset/begin + TLAS build), `assemble_camera_
  and_lights` (upload lights/camera, TAA jitter, camera-cut detection, DOF),
  `dispatch_skin_and_cluster` (bone/skin upload + palette dispatch +
  cluster-cull), and `build_and_upload_instances` (the draw-command →
  `GpuInstance` translation, the sort-key contract, and the material/terrain
  reupload). Each takes `&mut self` and the subset of `FrameInputs` it needs;
  `draw_frame` becomes the orchestrating call sequence. Mechanical —
  preserve barrier/dispatch order verbatim per
  `feedback_speculative_vulkan_fixes.md`; needs a live-engine smoke run to
  confirm no behavioral drift, same caveat #2258/#2259's own commit messages
  recorded for their extractions.

---

### TD3-2026-08-24-02: `audit-scripting/SKILL.md` cites a test name `cee35507` renamed hours before this audit ran

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `.claude/commands/audit-scripting/SKILL.md:871`; the renamed
  test is `crates/scripting/src/translate/effects.rs:2125`
- **Status**: NEW
- **Age**: same-day — `cee35507`, 2026-08-24
- **Effort**: trivial
- **Description**: `audit-scripting/SKILL.md`'s regression-guard list reads:

  > `declines_on_unmodeled_effect`, `declines_on_control_flow`,
  > `empty_fragment_is_understood_as_noop` (`crates/scripting/src/translate/effects.rs`)

  `declines_on_control_flow` was renamed to `declines_unmodeled_conditional_guard`
  by today's `cee35507` ("Implement global variable management and
  conditional effects in quest fragments"), the same commit that landed the
  quest-fragment global-variable work this report's summary notes as
  otherwise clean. The other two names in the same list
  (`declines_on_unmodeled_effect`, `empty_fragment_is_understood_as_noop`)
  are unaffected and still correct.
- **Evidence**:
  ```
  $ grep -n "fn declines_on_control_flow\|fn declines_unmodeled_conditional_guard" \
        crates/scripting/src/translate/effects.rs
  2125:    fn declines_unmodeled_conditional_guard() {

  $ git show cee35507 -- crates/scripting/src/translate/effects.rs | grep -n "fn declines"
  208:-    fn declines_on_control_flow() {
  248:+    fn declines_unmodeled_conditional_guard() {
  ```
- **Impact**: Low — a reader following the skill's regression-guard pointer
  to verify scripting coverage gets a `grep` miss on one of three named
  tests. Caught early (same day) because this sweep ran right after the
  rename; left alone it becomes indistinguishable from the older instances
  of this class the mechanism findings (#3052, TD4-2026-08-20-01) already
  describe.
- **Related**: #3052 / TD4-2026-08-20-01 (the general mechanism — this is a
  fresh instance, not itself a mechanism finding).
- **Suggested Fix**: One-word edit: `declines_on_control_flow` →
  `declines_unmodeled_conditional_guard` at `audit-scripting/SKILL.md:871`.

---

## Verified Clean

Recorded so the next sweep does not re-derive them.

- **Dim 1 — #1749 fixed.** `context/mod.rs`'s 1025-LOC `VulkanContext::new()`
  constructor (the finding's original subject) was extracted to
  `context/init.rs`/`context/teardown.rs` by `6fad32ac` ("finish extracting
  VulkanContext::new() into init.rs/teardown.rs"). `mod.rs` production LOC
  dropped 4059→2306. Still over the 2000-LOC primary-bucket threshold, but
  the specific constructor this issue tracked is gone — do not re-file
  against the same subject; #2977 (the other Dim-1 tracking issue) is
  likewise closed with its own membership table current.
- **Dim 2 — coordinate-flip consolidation holds.** `crates/nif/src/import/coord.rs`
  and `crates/nif/src/anim/coord.rs` (the canonical homes `_audit-common.md`
  names) are both thin wrappers over the true single source,
  `crates/core/src/math/coord.rs::{zup_to_yup_pos, zup_to_yup_quat_wxyz}`
  (consolidated under #1044/TD3-002). `crates/spt/src/import/mod.rs` calls
  the `byroredux_core` helper directly rather than reimplementing it. No
  leak found.
- **Dim 2 — spot-checked today's scripting delta.** `crates/scripting/src/trigger.rs`
  (+602 LOC today: actor-gated quest triggers, tethered-horse detection) has
  one named constant (`TETHERED_HORSE_TRIGGER_RADIUS = 96.0`), no duplicated
  scaffolding, and its largest function (`trigger_detection_system`, ~180
  LOC) stays under the Dim-1 extraction trigger. Not a full sweep of the
  ~4500-line same-day scripting delta, but the sample is clean.
- **Dim 3 — actor-value key-space doc fixed.** `4e1afcbe`'s "unify actor
  value key space" rewrote `crates/core/src/ecs/components/actor_values.rs`'s
  module doc to state Skyrim authors built-in values as AVIF records too
  (per #2987) and drop the retired "Skyrim engine-enum index" framing —
  independently verified as the exact fix `REG-2026-08-20-D6-01` (#3216)
  asked for, though this audit does not close that issue (out of dimension:
  regression tracking is `/audit-regression`'s).
- **Dim 7 — `WATERLINE_HYSTERESIS` duplication fixed.** Consolidated to
  `crates/core/src/ecs/components/water.rs:59` per #3209's suggested fix;
  the second declaration in `byroredux/src/systems/water.rs` is gone.
- **Dim 4 — validate gate passes.** `_audit-validate.sh`: 1466 refs across 30
  skill files, 0 stale paths, 18 advisory symbols (all triaged in this
  report's "Premises investigated" section — one real, filed as
  TD3-2026-08-24-02; the rest benign or already tracked).
- **Dim 5 — 0 findings, unchanged composition.** All 20 marker hits remain
  the documented exclusion classes (ESM `XXXX` protocol tag, upstream-
  reference `FIXME`, prose about closed TODOs). Identical count and
  composition to every prior sweep back to 2026-08-16.
- **Dim 6 — 0 findings.** `unimplemented!`/`todo!()`/`panic!("not ` remain at
  0 workspace-wide. All `stub`/`placeholder`/`not yet` comment hits describe
  intentional design (best-effort parser capture, SpeedTree billboard
  fallback, Vulkan lifecycle notes).
- **Dim 8 — dead code, 69 `allow(dead_code)` sites (was 60).** The one new
  site checked in detail — `crates/core/src/ecs/lock_tracker.rs:56` (added by
  today's `WindField` scheduler-access commit, 278 LOC changed in this file)
  — is `#[cfg_attr(not(debug_assertions), allow(dead_code))]`, the
  documented release/debug field-parity pattern the SKILL explicitly
  excludes. `quest.rs`'s cluster grew 24→25 (still the tracked #2982 ALIAS_
  FLAG_* family — 20/25 unreachable, not a new distinct cluster).
  `env_translate.rs`'s one site (`INHERIT_MAP`, offset 278) is the same
  documented parsed-but-unconsumed protocol bit from every prior sweep.
- **Dim 9 — `#[ignore]`, 171 (rust-only; was 154).** Sampled growth is
  concentrated in water-test buildout, consistent with the WATAL-heavy
  delta; none inspected guards a closed CRITICAL/HIGH fix. The bare-recipe
  503-count false alarm (Existing #2262) is called out above so it is not
  mistaken for a fresh regression by the next reader of this report.

---

## Deferred

| Finding | Gating reason |
|---|---|
| `constants.rs`'s "well inside the 4 GB total VRAM budget" phrasing | Not re-litigated — a prior audit already identified the "4 GB" vs the documented 6 GB RT-minimum baseline as a one-line pending edit (`AUDIT_RENDERER_2026-08-12b.md`); out of this report's evidence chain to re-verify its current state. |
| Full per-file sweep of the ~4500-line same-day `crates/scripting` delta (quest fragments, globals, trigger gating) | Budget — this is a single-agent, no-fan-out run against a comprehensive 9-dimension scope; one representative file was sampled and came back clean. A future `/audit-scripting` or a focused `/audit-tech-debt --focus 1,2,7` pass over just this delta would give full coverage. |
| `docs/engine/memory-budget.md`'s Instance/Material SSBO row *recomputation* (not just the byte-size fix) | Folded into TD3-2026-08-24-01's suggested fix rather than split out — recomputing both rows is part of the same edit. |

---

## Deduplication Record

Baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 500 --state
all --label tech-debt --json number,title,state` (returned as one compact
JSON line — 77 of the entries `state=="OPEN"`), all 21 prior
`AUDIT_TECH_DEBT_*.md` reports (newest read in full:
`AUDIT_TECH_DEBT_2026-08-20.md`), and targeted `gh issue list --search`
queries per candidate finding.

**Checked and confirmed still OPEN, not re-filed:**

| Subject | Issue |
|---|---|
| `GpuCamera` grew 336→352 B, five doc sites still say 336 | #3201 — directly related to this report's TD3-2026-08-24-01 (same failure shape, next struct) |
| `_audit-validate.sh` doesn't inspect `docs/engine/*.md` | #3202 — the structural fix that would have caught both #3201 and TD3-2026-08-24-01 |
| Validate gate's case/negation blind spots | #3197 fixed the two documented blind spots; TD3-2026-08-24-01 notes a third (self-referential stale-citing-stale) without filing it separately |
| `IsCollisionOnly` marker removed, docs still reference it | #2638 |
| `resolve_water_material` 522-LOC function | tracked via #2977's family / the 08-20 report's TD1-2026-08-20-01 |
| `ActionState::is_held` redundant test-only allow | #2981 |
| `ALIAS_FLAG_*` 20/25 unreachable | #2982 |
| Shader-include allow-list gaps (`presentation.frag`, `water.vert`) | #2984 + TD9-2026-08-20-01 |
| `#[ignore]` bare-recipe false-regression signal | #2262 |
| `volumetrics.rs` >2000 production LOC | #2256 |

**Checked and confirmed CLOSED / already fixed, verified against live code
rather than trusted from the issue title:**

| Subject | Issue | Verification |
|---|---|---|
| `VulkanContext::new()` 1025-LOC constructor | #1749 | `context/mod.rs` shrank 4059→2306 prod LOC via `init.rs`/`teardown.rs` extraction (`6fad32ac`) |
| Dim-1 recipe measured total LOC, not production | #2974 | `prod_loc` present and correctly discriminating in this run |
| 08-16's seven newly-crossed >2000-LOC files | #2977 | membership table re-verified, matches |
| `WATERLINE_HYSTERESIS` declared twice | #3209 | single declaration now, in `core/water.rs` |
| ActorValues doc still declaring the retired Skyrim engine-enum key space | (REG-2026-08-20-D6-01 / #3216, not tech-debt-owned) | fixed by `4e1afcbe`, verified via diff |
| `bindings.glsl` `GpuMaterial` comment cites retired 348 B | #3240 | fixed in that file; the *same* stale number recurred in three *other* files after `GpuMaterial`'s next growth — see TD3-2026-08-24-01 |

---

## Next Step

```
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-08-24.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=1 LOW=2
