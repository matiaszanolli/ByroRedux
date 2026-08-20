# Tech-Debt Audit — 2026-08-20

**Depth**: deep · **Dimensions**: all 9 · **Sweep**: `comprehensive` audit-suite
· **Delta**: 335 commits since `AUDIT_TECH_DEBT_2026-08-16.md`, near-monothematic
session-70 WATAL water work.

## Scope

Whole workspace (24 crates + `byroredux/`), weighted toward the water delta as
dispatched: `crates/plugin/src/esm/records/misc/water.rs`,
`byroredux/src/env_translate.rs`, `crates/renderer/src/vulkan/water.rs`,
`crates/core/src/ecs/components/water.rs`, `byroredux/src/render/water.rs`,
`byroredux/src/cell_loader/water.rs`, `crates/physics/src/water.rs`,
`byroredux/src/systems/water.rs`, `byroredux/src/commands/water.rs`,
`docs/engine/watal.md`, plus `crates/renderer/shaders/water.{vert,frag}`.

Un-owned subsystems examined incidentally: the gameplay slice
(`byroredux/src/combat.rs` — carried, all findings already filed),
`crates/mod-runtime`, `crates/hkx`. **Not** examined: `crates/facegen`,
`crates/fsr3-sys`, `crates/debug-server` / `debug-protocol`.

No cargo was run (suite rule). All analysis is static: grep, `git log -S`,
`git blame`, and the `prod_loc` helper from the SKILL's Phase 1.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| MEDIUM | 3 |
| LOW | 6 |
| **Total** | **9** |

Per-dimension yield (every dimension enumerated, including clean ones):

| Dim | Area | Findings |
|---|---|---|
| 1 | File / Function / Module Complexity | **1** (LOW) |
| 2 | Logic Duplication | **1** (LOW) |
| 3 | Stale Documentation & Comments | **3** (1 MEDIUM, 2 LOW) |
| 4 | Audit-Finding Rot | **3** (2 MEDIUM, 1 LOW) |
| 5 | Stale Markers (TODO/FIXME/HACK/XXX) | **0 — CLEAN** |
| 6 | Stub & Placeholder Implementations | **0 — CLEAN** |
| 7 | Magic Numbers & Hardcoded Constants | **1** (LOW) |
| 8 | Dead Code & Backwards-Compat Cruft | **0 — CLEAN** (all live delta hits deduped) |
| 9 | Test Hygiene | **1** (LOW) |

**Headline**: `GpuCamera` grew **336 → 352 bytes** on `8e7582ed` (2026-08-16) and
the size was never propagated out of `gpu_types.rs`. Five documentation sites
still say 336 B — including the *heading and field table* of
`docs/engine/shader-pipeline.md`, which `_audit-common.md` designates as the
authority for "exact byte layouts", and `docs/engine/renderer.md`, which names a
**test that no longer exists** (*gpu_camera_is_336_bytes*) as the pin. This is a
direct regression of the 08-16 report's own "Verified Clean" line
(*"`GpuCamera` 336 B … consistent across every live doc comment and pinned
test"*), and it is the same failure mode as the `GpuMaterial` 300 → 348 B
incident that motivated the validate gate's symbol advisory in the first place.

The second theme is that the guard built to catch exactly that
(`_audit-validate.sh`'s symbol advisory) **structurally cannot see it**, for two
independently-verified reasons, and its file glob excludes `docs/engine/` where
all five stale sites live.

The water delta itself is in better debt shape than 335 commits of
monothematic work would predict: zero new markers, zero `unimplemented!`, zero
new stubs, shader `#define` provenance intact. Its real debt — the mesh-water
composition block, the `foam_strength` literals, the `GpuWaterParams` lockstep
gap, the `memory-budget.md` volumetrics row, the duplicated audio filter
block, the archive-backed UI path — was **already found and filed by six
sibling audits in this same sweep**, and is deduped below rather than
double-reported.

### Premises that did NOT survive verification

Investigated, could not be sustained, deliberately **not** reported:

- *"The `WaterKind` name classifier is duplicated with divergent behaviour —
  the WATR path maps `waterfall`/`falls` to `River` while the mesh path maps
  them to `Waterfall`."* The divergence is real but **deliberate and
  documented**: `byroredux/src/env_translate.rs:889-911` carries a 22-line
  rationale — cell planes are always horizontal, the `Waterfall` shader mode is
  for vertical sheet geometry the cell loader never spawns, and Skyrim ships
  many horizontal WATR records named `WaterFallingPool` /
  `WaterRiverFallingSlow` that the pre-fix heuristic painted with fizz foam
  across whole exterior cells. Demotion to `River` is the fix, not the bug.
  (The *token-set* drift — `canal` exists only on the mesh path — and the
  duplicated classifier itself are covered by AUDIT_LEGACY_COMPAT § LC-D5-02.)
- *"`byroredux_physics::{buoyancy_force, wind_force}` are dead — zero consumers
  outside their own module."* True of external call sites, but both are called
  from `crates/physics/src/water.rs:693` / `:710` inside the buoyancy scan and
  re-exported at `crates/physics/src/lib.rs:41-42`. The SKILL's Dim 8 exclusion
  for workspace-internal public API applies. Noted, not filed.
- *"`WATERLINE_HYSTERESIS`'s duplication is an oversight."* It is knowing —
  `crates/physics/src/water.rs:216-220` names the other copy. Only the *stated
  reason* fails (see TD7-2026-08-20-01), so the finding is narrower than first
  drafted.
- *"`GpuInstance` / `GpuMaterial` / `Vertex` / `GpuTerrainTile` doc sizes have
  drifted too."* They have not: 128 B, 348 B, 104 B and 96 B are consistent
  across every live doc comment and pinned test. Only `GpuCamera` moved.
- *"`presentation.frag`'s shader-constants omission widened."* It did, but by a
  *different* file (`water.vert`), which is why TD9-2026-08-20-01 is filed as a
  distinct instance rather than a restatement of #2984.

---

## Baseline Snapshot (for the next audit's diff)

```
TODO/FIXME/HACK/XXX:      20   (0 real — all protocol / upstream-ref / prose; unchanged)
allow(dead_code):         60   (was 58; 24 still the one ALIAS_FLAG_* block, #2982)
unimplemented!/todo!():    0   (unchanged)
#[ignore] tests:         154   (crates + byroredux + tools; was 140. All data/GPU gated)
files >2000 PRODUCTION LOC: 4  (unchanged membership — see below)
files >2000 TOTAL LOC:    16   (was 11; +5, four pre-existing + misc/water.rs new)
GLSL shaders:             25 header consumers (was 21 named); allow-list lists 16
shader #define outside generated header: 5 include guards + 1 alias macro (clean)
```

Dim-1 **production** bucket (the dimension's actual subject) — membership is
**unchanged** from the SKILL's 2026-08-19 orientation, all four already filed:

| Prod LOC | Total LOC | File | Issue |
|---|---|---|---|
| 4059 | 4488 | `crates/renderer/src/vulkan/context/mod.rs` | #1749 |
| 3496 | 4730 | `crates/renderer/src/vulkan/context/draw.rs` | #2977 |
| 2774 | 3456 | `crates/renderer/src/vulkan/volumetrics.rs` | #2256 |
| 2013 | 2021 | `crates/renderer/src/texture_registry.rs` | #2977 |

The **total-LOC** secondary bucket gained one genuinely new member in this
delta — `crates/plugin/src/esm/records/misc/water.rs` (2081 total, **1418
production**). Below the 2000-production threshold, so per the recipe it is
**reported, not filed**. The other four newly-listed files
(`byroredux/src/env_translate.rs` 3216/1405, `misc/world.rs` 2116/1204,
`crates/physics/src/world.rs` 2115/1044, `byroredux/src/cornell.rs` 2114/1637)
were already on the SKILL's secondary list and all remain majority-test or
under threshold.

`prod_loc` behaved correctly on every file checked; no disagreement with the
SKILL's filed evidence this cycle (the `texture_registry.rs` discrepancy the
SKILL documents is resolved in the SKILL's favour — 2013 production).

---

## Top Quick Wins (trivial, ≤30 min each)

1. **TD3-2026-08-20-01** — replace `336` with `352` at five doc sites and add
   the missing `render_debug` row to `shader-pipeline.md`'s `GpuCamera` table.
   Rename the cited test to `gpu_camera_is_352_bytes`. Highest value per minute
   in this report.
2. **TD4-2026-08-20-02** — delete the two "swimming/drowning are unbuilt"
   claims from `.claude/commands/audit-physics/SKILL.md` and
   `.claude/commands/_audit-common.md`. One of them tells the auditor to
   *"confirm absence rather than reporting it"* for code that shipped.
3. **TD9-2026-08-20-01** — add `water.vert` to
   `affected_shaders_include_constants_header`. One tuple.
4. **TD3-2026-08-20-02** — correct the `GpuWaterParams::uv_offset` doc comment:
   `zw` are not reserved, and cell WATR does not upload zero.
5. **TD4-2026-08-20-03** — widen `_audit-validate.sh`'s symbol-advisory regex to
   `[A-Za-z][A-Za-z0-9_]{6,}`. Currently one character wide of catching #3052.
6. **TD7-2026-08-20-01** — hoist `WATERLINE_HYSTERESIS` next to
   `WEATHER_SCROLL_PER_BU_PER_S` in `crates/core/src/ecs/components/water.rs`.

## Top Medium Investments

1. **TD4-2026-08-20-01** — fix the symbol advisory's two blind spots (case, and
   symbols cleared by a *negative* assertion). Both are demonstrated against a
   live OPEN issue (#3052) in the finding.
2. **TD4-2026-08-20-03** — extend the advisory's file glob to
   `docs/engine/*.md`. Running it there right now surfaces
   *gpu_camera_is_336_bytes*, i.e. it would have caught TD3-2026-08-20-01 on
   day one.
3. **TD1-2026-08-20-01** — decompose `resolve_water_material` (522 LOC, one
   495-line `if let` arm) along its six natural field groups.
4. **TD2-2026-08-20-01** — replace `render/water.rs`'s inline gust/direction/
   scale math with the `weather_wave_adjustment` call the two other consumers
   already use.
5. Carry-over: **#2984** (shader-include allow-list) and **#2977** (oversized
   files) are the two OPEN items with the widest ongoing blast radius.

---

# Findings

## MEDIUM

### TD3-2026-08-20-01: `GpuCamera` grew 336 → 352 B four days ago; five doc sites still say 336, and one names a test that no longer exists

- **Severity**: MEDIUM
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/engine/shader-pipeline.md:193` (section heading),
  `:194-212` (field table, missing a row), `:380` (descriptor-set table);
  `docs/engine/memory-budget.md:37`; `docs/engine/renderer.md:269`, `:576-577`;
  `crates/renderer/src/vulkan/context/mod.rs:704`
- **Status**: NEW — and a **regression of the 08-16 report's own "Verified
  Clean" claim** on this exact subject
- **Age**: `8e7582ed` (2026-08-16) grew the struct; every site above predates it
- **Effort**: trivial
- **Description**: `8e7582ed` appended a `render_debug` `uvec4` to `GpuCamera`,
  taking it 336 → 352 B. `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:290-296`
  and the pinned test were both updated (`gpu_camera_is_352_bytes`), and
  `.claude/commands/audit-renderer/SKILL.md:115` was updated — where it
  explicitly instructs the reader to *"confirm they hold and match
  shader-pipeline.md"*. `shader-pipeline.md` was not updated. Neither were
  three other docs. The severity table's promotion trigger — *"Stale
  `GpuCamera`/`GpuInstance`/`GpuMaterial` size in a doc comment (lockstep-drift
  bait)"* — applies directly.

  The worst site is `docs/engine/renderer.md:576-577`, which cites
  *gpu_camera_is_336_bytes* — a test name that exists nowhere in the tree — and
  glosses it as *"the live 336-byte `GpuCamera` layout"*. A reader who follows
  the doc's own instruction to check the pin finds nothing, and the doc's
  parenthetical asserts the wrong number is live.

  `shader-pipeline.md`'s field table is worse than a wrong number: it *ends* at
  offset 320 (`render_origin`), so a reader building or auditing a `CameraUBO`
  mirror from that table produces a 336-byte struct and silently truncates
  `render_debug`.
- **Evidence**:
  ```
  $ grep -rn "fn gpu_camera_is" crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs
  132:fn gpu_camera_is_352_bytes() {
  136:    "GpuCamera must be 352 B (336 B + 16 B render_debug uvec4) …"

  $ grep -rn "gpu_camera_is_336_bytes" crates byroredux
  (no output)

  $ grep -rn "336" docs/engine | grep -i camera
  docs/engine/shader-pipeline.md:193:### `GpuCamera` — 336 bytes, uniform buffer (Set 1, Binding 1)
  docs/engine/shader-pipeline.md:380:| 1 | 1 | `UNIFORM_BUFFER` | `GpuCamera` (336 B) | …
  docs/engine/memory-budget.md:37:| Camera UBO | — | 1 | 336 B | 336 B | **672 B** |
  docs/engine/renderer.md:269:6. Update the camera UBO (`GpuCamera`, 336 bytes) — …
  docs/engine/renderer.md:576:> `gpu_instance_is_128_bytes_std430_compatible`, `gpu_camera_is_336_bytes`
  ```
  `docs/engine/shader-pipeline.md`'s last table row is
  `| 320 | 16 | render_origin | … |` — there is no `| 336 | 16 | render_debug |`
  row. Meanwhile `gpu_types.rs:290-296` correctly reads *"**352 bytes** …
  ten trailing `vec4` … then 336 → 352 B with the structured renderer-debug
  control."*
- **Impact**: `_audit-common.md`'s Key Reference Docs table names
  `docs/engine/shader-pipeline.md` as the authority for
  "`GpuCamera`/`GpuInstance`/`GpuMaterial`/`GpuLight` **exact byte layouts**"
  and tells every audit to *"prefer them over re-deriving facts from source"*.
  An auditor or contributor who follows that instruction gets a wrong size and
  a truncated field list for the single most widely re-declared GPU struct in
  the tree — six shaders mirror `CameraUBO`. Per
  *feedback_shader_struct_sync* this hand-mirrored pattern is the project's
  documented #1 source of silent GPU desync. Nothing is broken at runtime
  today (the `reflect.rs` `uniform_block_size_by_name` pin covers the shaders);
  the damage is that the reference material now actively teaches the wrong
  contract.
- **Related**: TD4-2026-08-20-03 (why no gate caught this — `docs/engine/` is
  outside `_audit-validate.sh`'s glob, and the drifted test name is exactly the
  class the symbol advisory was built for). Precedent: `GpuMaterial` 300 → 348 B,
  cited in `_audit-common.md:277-279` as the incident that justified the
  advisory. `context/mod.rs:704`'s "doesn't touch GpuCamera's 336 B layout" is
  a *historical* claim about #1023 and only needs re-tensing.
- **Suggested Fix**: Change 336 → 352 at all five doc sites; add the
  `| 336 | 16 | render_debug | … |` row to `shader-pipeline.md`'s `GpuCamera`
  table; rename the cited test in `renderer.md:576` to
  `gpu_camera_is_352_bytes`; re-tense `context/mod.rs:704` to "the then-336 B
  layout". Then adopt TD4-2026-08-20-03 so the next growth is caught
  mechanically.

---

### TD4-2026-08-20-01: the validate gate's symbol advisory is blind to `SCREAMING_SNAKE_CASE` and is cleared by *negative* assertions — both modes proven against OPEN #3052

- **Severity**: MEDIUM
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-validate.sh:169-208` (the advisory
  block); the demonstrating case is `.claude/commands/audit-safety/SKILL.md:257`
  and `crates/renderer/src/shader_constants.rs:1241`
- **Status**: NEW — #3052 is the *instance*; this is the *mechanism*. Filed
  separately per the precedent of TD4-2026-08-16-01 / #2974, which filed a
  recipe defect apart from the instances it produced.
- **Effort**: small
- **Description**: The dispatch asked me to verify rather than trust
  `0b9a0c9d`'s claim to have resolved the advisory symbol-drift flags. The
  claim is *true for what the gate can see* — the advisory prints **zero**
  entries at HEAD across 1422 refs in 30 skill files. But `/audit-safety` found
  `REFRACT_PASSTHRU_BUDGET` still backticked at `audit-safety/SKILL.md:257`
  while existing nowhere (#3052, OPEN). I reproduced why, and it is two
  independent blind spots, not one oversight:

  **(a) Case.** The needle regex is
  ``grep -rhoE '`[a-z][a-z0-9_]{6,}`' `` — anchored to a lowercase first
  character. Every `SCREAMING_SNAKE_CASE` constant is excluded *before* any
  existence check runs. That is the dominant naming convention for exactly the
  symbols audit skills cite most: budgets, limits and flag bits
  (`MAX_TOTAL_BONES`, `GLASS_RAY_BUDGET`, `MAX_MATERIALS`, `MAX_WATER_DRAWS`,
  `RESTIR_M_CAP`, `INSTANCE_FLAG_*`, `MAT_FLAG_*`). The skill files backtick
  **157** distinct uppercase symbols today; the advisory examines none of them.

  **(b) Negative assertions.** The existence check is
  `grep -qw "$sym" "$src_blob"` over every tracked `.rs` concatenated. A symbol
  whose *only* `.rs` occurrence is inside an assertion that it must **not**
  exist is therefore treated as live. `REFRACT_PASSTHRU_BUDGET` is precisely
  that case, so widening the regex alone would still not catch #3052.
- **Evidence**:
  ```
  $ .claude/commands/_audit-validate.sh
  Checked 1422 refs across 30 skill files.
  OK: all path references valid.          # zero advisory lines

  # (a) — run the SAME check with an uppercase-inclusive needle:
  $ git ls-files '*.rs' | xargs cat > /tmp/rs_blob
  $ grep -rhoE '`[A-Z][A-Z0-9_]{6,}`' .claude/commands/_audit-*.md \
        .claude/commands/audit-*/SKILL.md | tr -d '`' | sort -u > /tmp/upper
  $ wc -l < /tmp/upper
  157
  $ while read s; do grep -qw "$s" /tmp/rs_blob || echo "$s"; done < /tmp/upper
  BGSM_MODEL_SPACE_NORMALS
  BGSM_PBR
  FO4_ENV_SCALE
  RESTIR_M_CAP
  TECH_DEBT
  VERTEX_INPUT

  # (b) — REFRACT_PASSTHRU_BUDGET is absent from that list, because:
  $ grep -rn "REFRACT_PASSTHRU_BUDGET" crates byroredux
  crates/renderer/src/shader_constants.rs:1241:  !src.contains("REFRACT_PASSTHRU_BUDGET = 2"),
  ```
  Triage of the six: *FO4_ENV_SCALE* is a **genuine** hit —
  `audit-fo4/SKILL.md:110` backticks it while its own sentence says the name was
  replaced by `FO4_DLC_UPPER` under #1242, so the convention requires it be
  *italicised*. `BGSM_PBR` / `BGSM_MODEL_SPACE_NORMALS`
  (`audit-fo4/SKILL.md:143`) are mis-named — the real symbols are
  `MAT_FLAG_BGSM_PBR` / `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS`. `RESTIR_M_CAP` is
  a false positive of the `.rs`-only corpus (it lives in
  `crates/renderer/shaders/triangle.frag`). `TECH_DEBT` and `VERTEX_INPUT` are
  benign prose, the same class the existing `comprehensive` filter handles.
  So the widened check has a ~50 % true-positive rate — well inside the
  "advisory, not fatal" framing the block already documents.
- **Impact**: The advisory exists because *"`gpu_material_size_is_300_bytes`
  outlived a 300 → 348 B `GpuMaterial` change — a wrong number in a GPU layout
  contract"* (`_audit-validate.sh:158-159`). Four days ago `GpuCamera` did the
  same thing (TD3-2026-08-20-01) and the advisory again printed nothing. A
  guard that reports clean while the exact defect class it was built for is
  live is worse than no guard: it converts "nobody checked" into "the check
  passed", which is what `0b9a0c9d`'s closeout recorded.
- **Related**: #3052 (OPEN — the instance); TD3-2026-08-20-01 (the live
  recurrence); TD4-2026-08-20-03 (the file-glob half of the same gap);
  `_audit-common.md:270-279` (the convention this enforces).
- **Suggested Fix**: (1) Widen the needle to
  `` `[A-Za-z][A-Za-z0-9_]{6,}` `` and add `TECH_DEBT` / `VERTEX_INPUT` to the
  benign list. (2) Build the blob from lines that are not negations — cheapest
  correct form is to exclude lines matching `!src.contains(` and
  `!.*contains(` from `$src_blob`, so a "must not exist" assertion stops
  counting as evidence of existence. (3) Extend the corpus to
  `git ls-files '*.rs' '*.glsl' '*.vert' '*.frag' '*.comp'`, which removes the
  `RESTIR_M_CAP` class of false positive and lets several ad-hoc entries be
  dropped from the filter list.

---

### TD4-2026-08-20-02: two audit-infrastructure files still call character swimming/drowning unbuilt, and one instructs the auditor to "confirm absence rather than reporting it" — it misdirected today's physics audit

- **Severity**: MEDIUM
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-physics/SKILL.md:290-291` (the Dim 6
  checklist line) and `:60-63`; `.claude/commands/_audit-common.md:134` (the
  `docs/engine/watal.md` row of the Key Reference Docs table)
- **Status**: NEW
- **Age**: the claim was accurate when written (2026-08-13); the code landed in
  this delta
- **Effort**: trivial
- **Description**: Character swimming and bounded drowning damage shipped in
  this delta. `byroredux/src/systems/character.rs` now carries
  `swimlevel_reached` (`:956`), `SWIM_HEIGHT_SCALE` (`:953`),
  `swim_vertical_velocity` (`:964`), `advance_breath` (`:999`),
  `DROWNING_DAMAGE_PER_SECOND` (`:1001`) and `apply_player_drowning_damage`
  (`:1027`), wired into `character_controller_system` at `:236-253` and
  `:474-483`.

  `docs/engine/watal.md` — the spec — **was** refreshed and is correct
  (`:22-23` "Character swimming and bounded drowning damage are live";
  `:408` "character swim core live"; `:617-618`). The two audit-infrastructure
  files were not. The `audit-physics` line is the damaging one because it is
  not merely stale prose, it is an *instruction*:

  > `- Character swimming/drowning are **unbuilt** (WATAL open items). Confirm absence rather than reporting it.`

  An auditor following it skips the newest, least-reviewed code in the
  subsystem it is dispatched to audit.
- **Evidence**: This is not hypothetical — it nearly fired today.
  `docs/audits/AUDIT_PHYSICS_2026-08-20.md:80-88` opens a section titled
  *"WATAL spec drift worth recording (not a bug)"*:

  > `docs/engine/watal.md`'s open-items list — and this audit skill's
  > Dimension 6 instruction to *"confirm absence rather than reporting it"* —
  > say character swimming/drowning are unbuilt. **They are built as of this
  > delta** … **Two findings below are *in* that new code.**

  Those two are PHYS-D4-2026-08-20-03 (a drowned actor keeps its AI and never
  ragdolls) and PHYS-D5-2026-08-20-06 (frame-rate-dependent swim damping).
  The physics audit caught the stale instruction and overrode it; a less
  skeptical run would have returned "confirmed absent" and lost both. The
  physics audit explicitly hands the skill-text half to this audit.
  (Its secondary claim that `watal.md`'s open-items list is also stale does
  **not** hold — I re-read `:18-29`, `:218-227` and `:405-425`; all three say
  swim/drown are live. The drift is confined to the two files above.)
- **Impact**: A stale audit baseline that demonstrably misled an audit inside
  the last 90 days — the severity table's explicit MEDIUM promotion trigger.
  The blast radius is every future `/audit-physics` run until fixed, over the
  subsystem's newest code.
- **Related**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` § "WATAL spec drift";
  PHYS-D4-2026-08-20-03, PHYS-D5-2026-08-20-06 (the two findings the
  instruction would have suppressed).
- **Suggested Fix**: Delete the `audit-physics/SKILL.md:290-291` bullet
  outright (the code is now in scope like any other) and update `:60-63` plus
  `_audit-common.md:134` to list the *actual* remaining open items —
  water-walking, freezing, exact Skyrim DNAM-tail decode, cross-game visual
  smoke — which is what `watal.md:415-425` already says.

---

## LOW

### TD4-2026-08-20-03: `_audit-validate.sh` never inspects `docs/engine/` — the Key Reference Docs every audit is told to trust are outside both gates

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-validate.sh:78-81` (`skill_files`)
- **Status**: NEW
- **Effort**: trivial (glob change) + small (triaging the resulting noise floor)
- **Description**: The gate globs exactly two shapes —
  `.claude/commands/audit-*/SKILL.md` and `.claude/commands/_audit-*.md`. Yet
  `_audit-common.md:116-138` lists eighteen `docs/engine/*.md` files as
  *"the authoritative, code-verified reference for their domain"* and instructs
  every audit to *"prefer them over re-deriving facts from source"*. Those
  eighteen files are checked by nothing. All five stale sites in
  TD3-2026-08-20-01 live there.
- **Evidence**: Simulating the *existing, unmodified* advisory logic over
  `docs/engine/*.md` surfaces the drift immediately:
  ```
  $ grep -rhoE '`[a-z][a-z0-9_]{6,}`' docs/engine/*.md | tr -d '`' | sort -u | wc -l
  1184
  $ while read s; do grep -qw "$s" /tmp/rs_blob || echo "$s"; done < /tmp/doc_syms
  …
  gpu_camera_is_336_bytes          ← docs/engine/renderer.md:576
  …
  ```
  The symbol is backticked, lowercase, ≥7 chars — squarely inside the advisory's
  *current* needle. Only the file glob kept it invisible. The raw noise floor
  over `docs/engine/` is ~60 entries, dominated by two classes the gate already
  filters (git short hashes) or can filter trivially (nif.xml field names such
  as `bhk_rigid_body`, `has_animation_notes`), plus game asset names
  (`glasspitcher`, `citycydoniamainlevel`).
- **Impact**: Two of this report's three MEDIUMs are doc drift in files the gate
  cannot see, and one of those (`GpuCamera`) is a GPU layout contract. The
  #1114 gate closed the recurring `TD7-*` stale-path family for *skills*; the
  same family is unpoliced for *reference docs*, and the reference docs are
  what audits are told to believe.
- **Related**: TD3-2026-08-20-01 (what it would have caught),
  TD4-2026-08-20-01 (the case/negation half of the same gap), #1114.
- **Suggested Fix**: Add `docs/engine/*.md` to `skill_files`. Keep the
  *symbol* half advisory as it is today, but extend the *path* half to it as
  well — `docs/engine/` carries many relative markdown links which the existing
  `path_exists` suffix matcher already handles. Add filters for git short
  hashes (already present) and a `nif_` / `bhk_` prefix class before enabling,
  so the first run is not a wall of noise.

---

### TD3-2026-08-20-02: `GpuWaterParams::uv_offset`'s doc says `zw` are reserved and cell WATR uploads zero — both halves were made false by yesterday's commit, and the uploader's own comment says the opposite

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/renderer/src/vulkan/water.rs:136-138` (the doc comment);
  contradicted by `byroredux/src/render/water.rs:332-340` and by both GLSL
  mirrors — `crates/renderer/shaders/water.frag:123-125`,
  `crates/renderer/shaders/water.vert:130-132`
- **Status**: NEW
- **Age**: `1a428278` (2026-08-20) claimed the lanes; the doc predates it
- **Effort**: trivial
- **Description**: The Rust struct doc reads:
  ```rust
  /// xy = authored mesh-water UV offset; zw are reserved for future
  /// transform terms. Cell WATR surfaces upload zero.
  pub uv_offset: [f32; 4],
  ```
  Both clauses are now false. `1a428278` claimed `.z` for the mesh-water
  flow-map bindless index (bit-cast) and `.w` for its authored tile scale, and
  updated **both** GLSL mirrors to say so — but not the Rust struct doc, which
  is the declaration everyone edits first. Cell WATR surfaces upload
  `u32::MAX` and a neutral scale, not zero.

  The uploader carries a correct comment fourteen lines above the write, so the
  repository now states both versions:
  ```rust
  // byroredux/src/render/water.rs:332-340
  // z carries the optional mesh-water flow-map bindless index as
  // integer bits; w carries its authored tile scale. Cell WATR
  // surfaces upload the u32::MAX index and neutral scale.
  uv_offset: [ mat.uv_offset[0], mat.uv_offset[1],
               f32::from_bits(mat.flow_map_index), mat.flowmap_scale ],
  ```
  A second, smaller instance in the same struct: `absorption`
  (`water.rs:121-123`) is documented as *"Starfield per-channel
  color-absorption ranges … zero triplet is the legacy scalar-fog sentinel"*
  with no mention of `.w`, while the uploader writes
  `precipitation * rain_response` there (`render/water.rs:312-321`) and
  `water.vert:105` documents it as `w = precipitation`. `water.frag` is silent
  on `.w` too, so two of the three sites under-document a live lane.
- **Evidence**:
  ```
  $ git log --oneline -S'f32::from_bits(mat.flow_map_index)' -- byroredux/src/render/water.rs
  1a428278 feat(water): enhance water rendering with depth-dependent alpha controls   (2026-08-20)
  $ grep -n "uv_offset" crates/renderer/shaders/water.vert
  131:    // xy = authored mesh-water UV offset; z = flow-map index bit-cast;
  132:    // w = authored flow-map scale.
  ```
- **Impact**: `zw are reserved` is an active invitation to reuse a live lane.
  The struct is 352 B against a 64 KiB UBO with **64 bytes** of real headroom
  (see AUDIT_SAFETY_2026-08-20 § the `MAX_WATER_DRAWS` finding), so "there are
  two spare floats right here" is exactly the wrong thing for the next author
  to believe — the alternative to reusing them is adding a `vec4`, which
  overflows the buffer on essentially every device. Same failure shape as
  `VolumetricsParams.render_origin.w` (#1928) and
  `GpuCamera.render_origin.w` (#2164), both of which
  `docs/engine/shader-pipeline.md:212` calls out by name as *"Not a free slot"*.
- **Related**: REN-D15-01 (AUDIT_RENDERER_2026-08-20) establishes that the
  three declarations' *field names and order* are identical at HEAD — this
  finding is about the *semantics documented for the lanes*, which that
  field-name diff cannot see, and which no sibling audit covers.
- **Suggested Fix**: Copy the uploader's comment onto the struct field, and
  extend `absorption`'s doc with `w = precipitation × rain_response`. While
  there, mark both as "not a free slot" in the same words
  `shader-pipeline.md` uses for the two prior instances.

---

### TD3-2026-08-20-03: `shader-pipeline.md`'s descriptor table stops at Set 2 Binding 0 — the water params UBO, the largest per-draw GPU contract in the delta, is in no document

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/engine/shader-pipeline.md:375-398` (the Set 0–2 descriptor
  table); the undocumented resource is `crates/renderer/src/vulkan/water.rs:316`
  (`.binding(1)`) / `:65-140` (`GpuWaterParams`)
- **Status**: NEW
- **Effort**: trivial
- **Description**: `_audit-common.md:121` designates `shader-pipeline.md` as the
  authority for *"descriptor set bindings (Set 0–2)"*. Its table's last row is
  `| 2 | 0 | STORAGE_IMAGE (R32_UINT) | Water caustic accumulator | water.frag |`.
  Set 2 Binding 1 — the `WaterParamsBlock` UBO, `GpuWaterParams[186]`, 352 B
  per record, ~65.5 KB, bound by both `water.vert` and `water.frag` — has no
  row. `GpuWaterParams` appears in **no** file under `docs/` at all, while its
  four siblings (`GpuCamera`, `GpuInstance`, `GpuMaterial`, `GpuLight`) each
  get a full offset/size/field table in the same document's "GPU Data Types"
  section.
- **Evidence**:
  ```
  $ grep -rn "GpuWaterParams" docs/
  (no output outside docs/audits/)
  $ sed -n '396,398p' docs/engine/shader-pipeline.md
  | 1 | 18 | STORAGE_BUFFER | Previous-frame rigid instance model matrices …
  | 2 | 0  | STORAGE_IMAGE (R32_UINT) | Water caustic accumulator | water.frag |
                                       ← table ends; no `| 2 | 1 |` row
  $ grep -n "binding(1)" crates/renderer/src/vulkan/water.rs
  316:            .binding(1)
  ```
- **Impact**: Documentation gap rather than rot, but it compounds two live
  findings: REN-D15-01 (the struct has three hand-mirrored declarations and no
  lockstep guard) and TD3-2026-08-20-02 (its lane semantics are documented
  inconsistently across those three). A `GpuWaterParams` offset table in the
  authoritative doc would give all three a single reference to diff against —
  and would make the 64-byte UBO headroom visible where someone will actually
  see it before adding a field.
- **Related**: REN-D15-01, TD3-2026-08-20-02, AUDIT_SAFETY_2026-08-20's
  `MAX_WATER_DRAWS` finding.
- **Suggested Fix**: Add the `| 2 | 1 | UNIFORM_BUFFER | GpuWaterParams[186] (352 B each, ~65.5 KB) | water.vert, water.frag |`
  row, and a `### GpuWaterParams — 352 bytes` subsection to "GPU Data Types"
  mirroring the `GpuCamera` table's format.

---

### TD2-2026-08-20-01: `render/water.rs` re-implements `weather_wave_adjustment` inline instead of calling it, so the renderer and the physics/gameplay sampler can silently diverge

- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `byroredux/src/render/water.rs:99-137`; the canonical function is
  `crates/physics/src/water.rs:323-350`
- **Status**: NEW — explicitly handed to this audit by
  `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md:598-601` (*"Real, and worth
  filing — but it is a duplication/divergence defect, not a concurrency one.
  Out of dimension; belongs to `/audit-tech-debt`"*)
- **Effort**: trivial
- **Description**: `byroredux_physics::weather_wave_adjustment(world, t)`
  returns `([f32; 2], f32)` — the weather scroll vector and the wave-amplitude
  multiplier — and is the declared single source for that pair; its doc says
  *"gameplay, camera effects, and buoyancy all agree with the rendered crest"*.
  Two consumers call it (`byroredux/src/systems/water.rs:180`,
  `byroredux/src/systems/character.rs:915`). The third — the renderer, i.e. the
  end that actually produces the crest the other two are meant to agree with —
  recomputes the whole body inline over 39 lines. All five steps are
  reproduced: the gust sine, the one-sided `max(0.0)` finite clamp, the
  `1e-6`-thresholded direction normalise with the `[1.0, 0.0]` fallback,
  `scroll = dir · gust · WEATHER_SCROLL_PER_BU_PER_S`, and
  `scale = 1 + clamp(gust / MAX_WIND_SPEED, 0, 1) · 0.5`.

  `byroredux` already depends on `byroredux-physics` and the function is
  already re-exported at `crates/physics/src/lib.rs:42`, so the consolidation
  is a two-line substitution with no new dependency edge.
- **Evidence**: field-by-field, the two are currently **identical** — I diffed
  them and found no drift, which is why this is LOW and not MEDIUM (the
  severity table's promotion needs a *divergent fix history*, and there is none
  yet).
  ```rust
  // crates/physics/src/water.rs:326-349          // byroredux/src/render/water.rs:99-137
  let gust = wind.speed                            let gust = weather_wind.speed
    + wind.gust_amplitude * (t * wind.gust_frequency * TAU).sin();     … identical …
  let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };       … identical …
  let len_sq = …; if len_sq.is_finite() && len_sq > 1.0e-6 { … } else { [1.0, 0.0] }
  let scroll = [dir[0] * gust * WEATHER_SCROLL_PER_BU_PER_S, …];       … identical …
  let scale = 1.0 + (gust / MAX_WIND_SPEED).clamp(0.0, 1.0) * 0.5;     … identical …
  ```
  Secondary, from the same concurrency note: `render/water.rs` acquires
  `WeatherDataRes` twice in eight lines (`:84`, `:91`) where the substitution
  would leave one site.
- **Impact**: CLAUDE.md's global policy is explicit — *improve existing code,
  never duplicate logic*. The specific hazard here is that WATAL is a
  **double-ended** layer whose entire premise is that both ends consume one
  canonical representation, and this is the seam between them. A retune of the
  0.5 amplitude coefficient or the `1e-6` direction threshold in one copy makes
  buoyancy and camera submersion track a crest the shader is not drawing. That
  the two ends can disagree is not speculative: #2888 (OPEN) is
  *"the two ends of WATAL disagree on which overlapping water plane wins"*, and
  #2870 / #2872 were both closed WATAL end-disagreement bugs.
- **Related**: `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md` § "Disproved /
  out-of-dimension" item 7 and 8; #2888, #2870, #2872;
  `docs/engine/watal.md` §4 item 4 (*"Physics is game-invariant"*).
- **Suggested Fix**: Replace `render/water.rs:99-137` with
  `let (weather_scroll, wind_wave_scale) = byroredux_physics::weather_wave_adjustment(world, time_secs);`
  and delete the now-unused `weather_wind` binding. Fold the second
  `WeatherDataRes` acquire into the first while there.

---

### TD1-2026-08-20-01: `resolve_water_material` is 522 LOC — one 495-line `if let` arm writing 40+ distinct `WaterMaterial` fields

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/env_translate.rs:535-1057`
- **Status**: NEW (the *file* is on the secondary bucket and covered by #2977;
  this is the function-level signal, which the SKILL notes is independent —
  *"file-level crossings and function-level splits are independent signals;
  don't assume one moves the other"*)
- **Effort**: medium
- **Description**: The EXAL water-translate boundary is one function whose body
  is, after five `let mut` accumulators, a single
  `if let Some(rec) = waters.get(&form) { … }` arm spanning 495 lines at brace
  depth up to 5. It is well past the SKILL's >200-LOC extraction trigger and is
  the largest function in the second-hottest file of the delta
  (`env_translate.rs`, 48 commits). Two of this sweep's water findings land
  inside it (the `foam_strength` literals at `:932`/`:947`, per NIFAL-D1-…-02),
  which is the practical cost: every water fix now edits the same 500-line
  block, and reviewing one means paging past the other 480 lines.
- **Evidence**:
  ```
  $ python3 fnlen.py byroredux/src/env_translate.rs | sort -rn | head -1
  522     byroredux/src/env_translate.rs:535     resolve_water_material
  # top-level statements in the body:
  let mut mat / kind / flow / normal_path / noise_paths;
  if let Some(form) = xcwt_form { … 495 lines … }
  let _ = SubmersionState::default();
  # 167 lines at indent ≥16, 75 at ≥20, 24 at ≥24; max brace depth 5
  ```
  The field writes group cleanly by responsibility — the function is long, not
  tangled, which is what makes it splittable:

  | Group | Fields written |
  |---|---|
  | Colour + fog | `shallow_color`, `deep_color`, `fog_near/far`, `day_*`, `night_*`, `underwater_*` (×4) |
  | Layer motion | `scroll_a/b/c`, `uv_scale_a/b/c`, `flowmap_scale` |
  | Noise + rain | `noise_falloff`, `rain_velocity/falloff/dampener/response/start_size` |
  | Specular + reflection | `reflectivity`, `reflection_tint`, `reflection_hdr_multiplier`, `specular_radius/magnitude`, `sun_specular_power`, `fresnel_f0` |
  | Kind + flow | `kind`, `flow`, `foam_strength`, `wave_amplitude/frequency` |
  | Texture paths | `normal_path`, `noise_paths[3]` |
- **Impact**: Maintenance cost only — no correctness claim. But this is the
  single translate boundary for every game's exterior water, it took 48 commits
  in four days, and the next water change will land in it too.
- **Related**: #2977 (the file-level bucket); NIFAL-D1-2026-08-20-02 (a finding
  *inside* this function); the Session-34/35 split precedent.
- **Suggested Fix**: Extract five private helpers along the table above —
  `resolve_water_colors(&mut mat, rec, …)`, `resolve_water_layer_motion`,
  `resolve_water_noise_and_rain`, `resolve_water_specular`,
  `classify_water_kind_and_flow` — each taking `&mut WaterMaterial` and
  `&WatrRecord`. `classify_water_kind_and_flow` is also the natural home for
  the `foam_strength` mapping NIFAL-D1-2026-08-20-02 wants hoisted, so the two
  fixes compose.

---

### TD7-2026-08-20-01: `WATERLINE_HYSTERESIS` is declared twice across the two ends of WATAL, and the stated reason for not sharing it is disproved by the constant declared 100 lines away

- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `byroredux/src/systems/water.rs:19` and
  `crates/physics/src/water.rs:221`; the canonical home is
  `crates/core/src/ecs/components/water.rs`
- **Status**: NEW
- **Effort**: trivial
- **Description**: Both ends of the WATAL layer declare
  `const WATERLINE_HYSTERESIS: f32 = 4.0;`. The physics copy documents the
  duplication and gives a reason:

  > *"Mirrors the camera submersion system's `WATERLINE_HYSTERESIS`
  > (`byroredux::systems::water`, #1450) — kept as a local constant because
  > that one is private to the binary crate."*

  The reason describes the current arrangement, not a constraint. The shared
  crate `byroredux-core` — which both `byroredux` and `byroredux-physics`
  already depend on — hosts precisely this class of constant, including one
  hoisted for exactly this purpose:
  ```rust
  // crates/core/src/ecs/components/water.rs:414
  pub const WEATHER_SCROLL_PER_BU_PER_S: f32 = 0.0015;
  ```
  consumed by `crates/physics/src/water.rs:343` **and**
  `byroredux/src/render/water.rs:133`. `WaterFlow::SPEED_MIN` / `SPEED_MAX` /
  `SPEED_RAPIDS` and `WaterFlow::speed_for_kind` sit in the same file for the
  same reason. Nothing prevents `WATERLINE_HYSTERESIS` joining them; it simply
  was not moved.
- **Evidence**:
  ```
  $ grep -rn "const WATERLINE_HYSTERESIS" crates byroredux
  byroredux/src/systems/water.rs:19:const WATERLINE_HYSTERESIS: f32 = 4.0;
  crates/physics/src/water.rs:221:const WATERLINE_HYSTERESIS: f32 = 4.0;
  ```
  Both are `4.0` at HEAD; there is no divergence today. The binary copy's own
  doc states the invariant that makes divergence a defect: *"the only
  constraint is that the vertical AABB acceptance below is extended by the
  **same** constant so the exit transition fires precisely at the band edge
  (#1450 / WAT-01)"* — and the physics copy *is* that AABB acceptance band, in
  a different crate, with no mechanism tying the two together.
- **Impact**: Retuning one leaves the camera's `head_submerged` hysteresis and
  the physics body↔water containment band at different widths, which is the
  #1450 WAT-01 flicker the constant exists to prevent, reappearing at a
  crate boundary where no test spans both. Latent, not live.
- **Related**: #2888 (OPEN — *"the two ends of WATAL disagree on which
  overlapping water plane wins"*, the same class); #1450 / WAT-01;
  TD2-2026-08-20-01 (the other cross-end WATAL duplication in this report).
- **Suggested Fix**: Move it to `crates/core/src/ecs/components/water.rs` as
  `pub const WATERLINE_HYSTERESIS: f32 = 4.0;` next to
  `WEATHER_SCROLL_PER_BU_PER_S`, carrying the binary copy's fuller doc comment
  (the unit note and the #1450 same-constant invariant), and import it at both
  sites.

---

### TD9-2026-08-20-01: `water.vert` became a `shader_constants.glsl` consumer in this delta and is not in the allow-list — the same guard hole as #2984, in a different file

- **Severity**: LOW
- **Dimension**: 9 — Test Hygiene
- **Location**: `crates/renderer/src/shader_constants.rs:415-469`
  (`affected_shaders_include_constants_header`); the unlisted consumer is
  `crates/renderer/shaders/water.vert:10`
- **Status**: NEW — related to **#2984** (OPEN, `presentation.frag`) but a
  distinct file that crossed into the consumer set *after* #2984 was filed, so
  fixing #2984 as written would not cover it
- **Age**: `c7561d74` (2026-08-19)
- **Effort**: trivial
- **Description**: `affected_shaders_include_constants_header` asserts that
  each shader in a hand-maintained 16-entry list contains
  `#include "include/shader_constants.glsl"`. There are **18** live non-header
  consumers. Two are missing: `presentation.frag` (#2984) and — new in this
  delta — `water.vert`, which gained the include in `c7561d74` in order to use
  `WATER_WATERFALL` at `:160`.

  This is the failure mode the guard exists to prevent, playing out
  in real time: a shader starts consuming a generated constant, nobody adds it
  to the list, and the guard reports green. Because the list is hand-maintained
  with no parity check against the actual `#include` set, it will keep drifting
  every time a shader picks up its first constant.
- **Evidence**:
  ```
  $ grep -rln "shader_constants.glsl" crates/renderer/shaders/ | grep -v include/ | wc -l
  18
  # entries in affected_shaders_include_constants_header: 16
  # set difference: presentation.frag (#2984), water.vert (new)

  $ grep -n "shader_constants.glsl" crates/renderer/shaders/water.vert
  10:#include "include/shader_constants.glsl"
  $ grep -n "WATER_WATERFALL" crates/renderer/shaders/water.vert
  160:    if (kind != WATER_WATERFALL) {
  $ git log --oneline -S'#include "include/shader_constants.glsl"' -- crates/renderer/shaders/water.vert
  c7561d74  (2026-08-19)
  ```
- **Impact**: If someone removes `water.vert`'s include while leaving
  `WATER_WATERFALL` in the body, the shader fails to compile — so this
  particular omission is caught by the build, not silently. The real cost is
  that the list is now demonstrably *behind* reality by two entries and has no
  self-check, which makes it the kind of guard that reads as coverage without
  providing it. `WATER_WATERFALL` is a `WaterKind` enum discriminant shared
  with `shader_constants_data.rs:430`; a divergence there is the lockstep class
  *feedback_shader_struct_sync* rates HIGH.
- **Related**: #2984 (OPEN — same guard, `presentation.frag`); the parity-check
  precedent is `dbg_bits_catalog_covers_every_dbg_constant`, cited in
  #2983 / TD9-2026-08-16-01 for the same "hand-maintained roster with no
  parity check" shape.
- **Suggested Fix**: Add the `water.vert` tuple now (one line). Then replace
  the hand-maintained list entirely: enumerate `crates/renderer/shaders/*.{vert,frag,comp}`
  at test time, and assert that any file mentioning a `shader_constants_data`
  symbol also carries the include — which makes the roster self-maintaining and
  closes #2984 in the same change.

---

## Verified Clean

Recorded so the next sweep does not re-derive them.

- **Dim 5 — Stale Markers: 0 findings.** All 20 `TODO|FIXME|HACK|XXX` hits are
  the documented exclusion classes, unchanged in count and composition from
  2026-08-16: the ESM `XXXX` extended-size tag (`crates/plugin/src/esm/reader.rs`
  ×9, `records/misc/magic.rs` ×3, `esm/cell/wrld.rs`, `records/misc/world.rs`),
  documentation *of an upstream reference implementation's* FIXME
  (`crates/bgsm/src/bgem.rs:137`, `crates/nif/src/blocks/bs_geometry.rs:596`,
  `records/misc/world.rs:216`), and prose referring to a closed TODO
  (`byroredux/src/scene.rs:1284`, `byroredux/src/groundcover_translate.rs:252`).
  `crates/renderer/shaders/` has **zero** marker hits. The MIT attribution
  block atop `triangle.frag` is intact. **335 commits added no new marker.**
- **Dim 6 — Stubs: 0 findings.** `unimplemented!` / `todo!()` / `panic!("not `
  remain at **0** workspace-wide. The 30 `stub|placeholder|not yet` comment
  hits are all prose describing intentional design (parser best-effort capture,
  SpeedTree billboard fallback, Vulkan "not yet bound" lifecycle notes), none a
  no-op implementation. Both new water console commands are wired:
  `WaterDumpCommand` / `WaterContactsCommand` are registered at
  `byroredux/src/commands/mod.rs:99-100` and neither no-ops.
- **Dim 7 — shader `#define` provenance holds.** Every `#define` in
  `crates/renderer/shaders/` outside five `#ifndef` include guards
  (`ray_origin.glsl`, `shadow_transport.glsl`, `shadow_common.glsl`,
  `mesh_id.glsl`, `caustic_kernel.glsl` — the last two added since 08-16) is
  generated from `shader_constants_data.rs`. The one remaining hit,
  `water.frag:139 #define push waterParams.params[drawPush.waterIndex]`, is an
  addressing alias, not a constant.
- **Dim 7 — GPU `#[repr(C)]` *code* literals are pinned and correct.**
  `gpu_instance_is_128_bytes_std430_compatible`, `gpu_camera_is_352_bytes`,
  `gpu_material_size_is_348_bytes`, `gpu_terrain_tile_is_96_bytes`,
  `selected_ray_probe_is_144_bytes_std430_compatible`, and the
  `const _: () = assert!(size_of::<GpuWaterParams>() == 352)` all hold. Only
  the *doc comments* drifted (TD3-2026-08-20-01), which is the Dim 3/Dim 7
  split the SKILL's cross-dimension rules prescribe.
- **Dim 3 structural half**: `.claude/commands/_audit-validate.sh` passes —
  1422 refs across 30 skill files, **0 stale paths**, **0** advisory symbols.
  The `TD7-*` stale-path family remains closed. But see TD4-2026-08-20-01 /
  -03 for why "0 advisory symbols" is not the reassurance it reads as.
- **Dim 8 — Dead code.** 60 `allow(dead_code)` (from 58). Every non-`quest.rs`
  site inspected is either `cfg`-gated, documented as a
  parsed-but-unconsumed protocol bit awaiting its first consumer
  (`env_translate.rs:278 INHERIT_MAP`), or an already-filed carry-over. No
  `// removed:` breadcrumbs, no `#[deprecated]`, no orphaned `_`-prefixed
  params. The three `fn *_unused*` hits remain legitimate names.
  `byroredux_physics::{buoyancy_force, wind_force}` have no *external* caller
  but are live inside the buoyancy scan (see disproved premises).
- **Dim 9 — `#[ignore]` triage**: all 154 are gated on installed game data, a
  Vulkan device, an audio device, or a multi-second corpus walk. None guards a
  closed CRITICAL/HIGH fix. The +14 versus 08-16 are concentrated in
  `crates/plugin/tests/parse_real_esm.rs` (20) and `crates/audio/src/tests.rs`
  (12), both data-gated.
- **`watal.md` is *not* stale on swim/drown**, contrary to a claim in
  AUDIT_PHYSICS_2026-08-20 — `:22-23`, `:218-224`, `:408-422` and `:617-618`
  all describe the swim core and drowning damage as live. The drift is confined
  to the two audit-infrastructure files in TD4-2026-08-20-02.

---

## Deferred

| Finding | Gating reason |
|---|---|
| `crates/plugin/src/esm/records/misc/water.rs` crossing 2000 total LOC (1418 production) | Below the production threshold the SKILL defines for Dim 1. Report-only per the recipe; re-file if production crosses 2000. |
| `CELL_CURRENT_UV_PER_BU_S = 0.0015` (`cell_loader/water.rs:386`) sharing a value with `WEATHER_SCROLL_PER_BU_PER_S = 0.0015` | Numerically identical, semantically distinct (cell-authored current vs. atmospheric wind). Per the no-guessing rule I will not assert they are the same quantity without a source; noted so a WATAL owner can decide. |
| `DISTURBANCE_BAND = 24.0` / `DISTURBANCE_RADIUS = 18.0` (`systems/water.rs:20-21`) carry no doc comment or citation, unlike every neighbouring water constant | Sits inside the disturbance-event slice that `watal.md:23-26` lists as newly landed. Worth a citation pass, but "undocumented" is not "wrong" and I have no source to check them against. |
| `crates/core/src/combat.rs` / `stealth.rs` zero-consumer state | #2962 CLOSED as an ownership question; the doc-rot half is #2979 (OPEN). Nothing new. |

---

## Deduplication Record

Baseline: `/tmp/audit/issues.json` (400 issues, #2671–#3103 — older numbers
carried on the prior report's word per the dispatch), all 23 prior tech-debt
reports (the 20 `AUDIT_TECH_DEBT_*.md` **and** the 3 hyphenated
`AUDIT_TECH-DEBT_*.md` the SKILL's Phase-1 glob still misses), and all 14
sibling audit reports already written in this sweep.

**All 12 findings from AUDIT_TECH_DEBT_2026-08-16 were published as
#2974–#2985.** Their live state:

| 08-16 Finding | Issue | State at HEAD |
|---|---|---|
| TD4-…-01 Dim 1 recipe measures total LOC | #2974 | **FIXED** — `prod_loc` is now in the SKILL's Phase 1 and Dim 1; both buckets verified working this cycle |
| TD6-…-01 `InputAction::Block` dead consumer arm | #2976 | CLOSED |
| TD3-…-01 feature-matrix native menus "Not planned" | #2975 | OPEN, unchanged |
| TD1-…-01 >2000-LOC set growth | #2977 | OPEN — production bucket membership unchanged (4 files) |
| TD2-…-01 raw-debug predicate hand-written | #2978 | OPEN, unchanged |
| TD3-…-02 `crates/core/src/combat.rs` "no consumer exists" | #2979 | OPEN — sentence still at `crates/core/src/combat.rs:9-11` |
| TD3-…-03 `combat_input_system` comment | #2980 | OPEN, unchanged |
| TD8-…-02 `ActionState::is_held` redundant allow | #2981 | OPEN — still at `interaction.rs:593` |
| TD8-…-01 `ALIAS_FLAG_*` 20/25 unreachable | #2982 | OPEN, unchanged |
| TD9-…-01 `alias_flags` tautological guard | #2983 | OPEN, unchanged |
| TD9-…-02 shader-include allow-list gap | #2984 | OPEN — **and widened by this delta**, see TD9-2026-08-20-01 |
| TD9-…-03 `skin_offsets` no hasher guard | #2985 | OPEN, unchanged |

TD4-2026-08-16-02 (three tech-debt reports invisible to the Phase-1 dedup glob)
was **not** published as its own issue and folded into #2422. Still live: the
three `AUDIT_TECH-DEBT_*.md` files exist, and `audit-tech-debt/SKILL.md:64`
still globs `AUDIT_TECH_DEBT_*.md` only. Carried, not re-filed.

**Skipped as already OPEN (issue exists, no new information):**

| Subject | Issue |
|---|---|
| `water.vert` stale "112-byte invariant" `GpuInstance` comment | #2763 — verified still present at `water.vert:47-48` |
| `water.frag` ampScale/freqScale sentinels, tautological test | #2787 |
| `WaterContact::depth` measured from body origin | #2887 |
| The two WATAL ends disagree on overlapping-plane selection | #2888 |
| `PhysicsWorld::add_force`/`apply_impulse`/`reset_forces` unused | #2889 |
| `RESERVOIR_LIGHT_MASK` no lockstep guard vs `MAX_LIGHTS` | #2778 |
| `GpuMaterial` GLSL lockstep pins names/order but not scalar type | #2688 |
| `REFRACT_PASSTHRU_BUDGET` backticked but nonexistent (the *instance*) | #3052 — TD4-2026-08-20-01 is the *mechanism*, filed separately |
| `feature-matrix.md` has no character/progression rows | #2961 |
| The 14 `active_package_is_*` PACK selectors are dead | #3042 |

**Skipped as covered by a sibling audit in this same sweep** — six of the seven
concrete leads in this audit's dispatch were verified and found already filed
elsewhere. Reporting them again would create duplicate GitHub issues:

| Subject (dispatch lead #) | Owner |
|---|---|
| (2) 39-line mesh-water composition block verbatim at `scene/nif_loader.rs:1027` + `cell_loader/spawn/mesh_instance.rs:724` | AUDIT_NIFAL_2026-08-20 § NIFAL-D1-2026-08-20-01 |
| (3) `WaterKind → foam_strength` literals ×3 ESM sites, absent on the NIFAL path (River 0.65 vs 0.20) | AUDIT_NIFAL_2026-08-20 § NIFAL-D1-2026-08-20-02 |
| (4) `memory-budget.md` volumetrics row 9× low vs its own detail section | AUDIT_RENDERER_2026-08-20 § REN-D16-01 **and** AUDIT_PERFORMANCE_2026-08-20 § PERF-D3-01 (which goes further — 6 volumes, a further 2.75×) |
| (5) `GpuWaterParams` three declaration sites, no lockstep guard, tautological `water_vertex_shader_keeps_the_full_material_array_stride` | AUDIT_RENDERER_2026-08-20 § REN-D15-01 |
| (6) Audio submersion low-pass construction block duplicated across both dispatch paths (the #2405 shape) | AUDIT_AUDIO_2026-08-20 — scored inside its MEDIUM filter finding, whose Suggested Fix already proposes `apply_underwater_filter` next to `apply_reverb_send` |
| (7b) `docs/engine/ui.md:39` + `ROADMAP.md:759` list the archive-backed menu path under Status with no engine caller | AUDIT_UI_2026-08-20 § UI-D5-02 (verified independently: `load_swf_from_resource_provider` and `load_swf_with_profile` have **zero** callers workspace-wide; `byroredux/src/scene.rs:1334` is the only SWF entry and uses `load_swf`) |
| Divergent `WaterKind` token sets between the two classifiers | AUDIT_LEGACY_COMPAT_2026-08-20 § LC-D5-02 |
| `water_material_from_mesh` flag-gate block | AUDIT_LEGACY_COMPAT_2026-08-20 § LC-D5-01 |
| `GpuWaterParams` finiteness / `MAX_WATER_DRAWS` vs `maxUniformBufferRange` | AUDIT_SAFETY_2026-08-20 |

Leads **(1)** and **(7a)** were explicitly handed to this audit by
AUDIT_CONCURRENCY_2026-08-20 and AUDIT_PHYSICS_2026-08-20 respectively, and are
filed here as TD2-2026-08-20-01 and TD4-2026-08-20-02.

---

## Next Step

```
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-08-20.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=3 LOW=6
