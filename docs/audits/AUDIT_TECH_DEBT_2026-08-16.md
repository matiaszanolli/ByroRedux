# Tech-Debt Audit — 2026-08-16

**Depth**: deep · **Dimensions**: all 9 · **Sweep**: `comprehensive` audit-suite

## Scope

Whole workspace (24 crates + `byroredux/`), with two deliberate emphases carried
in from this sweep's cross-audit lead:

1. **Guards and tests that are green by construction** — treated as a first-class
   debt dimension (folded into Dim 9, with spill-over into Dim 2 and Dim 4).
2. **The P2 gameplay slice** (`byroredux/src/{combat,inventory,settings_io}.rs`
   plus the action half of `byroredux/src/interaction.rs`), ~2.6 k LOC landed
   2026-08-15/16, which has **no owner audit skill** and had never seen a debt
   sweep.

Un-owned subsystems examined incidentally: gameplay slice (above),
`crates/hkx`, `crates/mod-runtime`, `crates/debug-ui`. **Not** examined:
`crates/facegen`, `crates/fsr3-sys`, `crates/debug-server` / `debug-protocol`.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 4 |
| LOW | 8 |
| **Total** | **12** |

Per-dimension yield (every dimension enumerated, including clean ones):

| Dim | Area | Findings |
|---|---|---|
| 1 | File / Function / Module Complexity | **1** (LOW) |
| 2 | Logic Duplication | **1** (LOW) |
| 3 | Stale Documentation & Comments | **3** (1 MEDIUM, 2 LOW) |
| 4 | Audit-Finding Rot | **2** (both MEDIUM) |
| 5 | Stale Markers (TODO/FIXME/HACK/XXX) | **0 — CLEAN** |
| 6 | Stub & Placeholder Implementations | **1** (MEDIUM) |
| 7 | Magic Numbers & Hardcoded Constants | **0 — CLEAN** |
| 8 | Dead Code & Backwards-Compat Cruft | **2** (both LOW) |
| 9 | Test Hygiene | **3** (all LOW) |

**Headline**: the codebase's *marker* and *magic-number* hygiene is genuinely
clean — all 20 `TODO|FIXME|HACK|XXX` hits are protocol tags or references to an
upstream project's FIXME, zero `unimplemented!`/`todo!()`, and every shader
`#define` outside three include-guards is generated from
`crates/renderer/src/shader_constants_data.rs`. The live debt has migrated to
**verification that cannot fail** and to **the audit infrastructure's own
measurement recipes**, which is where 6 of the 12 findings land.

The two most consequential findings are both Dim 4 — this audit's own Dimension 1
recipe measures a proxy (total file LOC) rather than the property it exists to
find (production complexity), and three prior tech-debt reports are structurally
invisible to the Phase-1 dedup scan because of a filename-separator drift.

### Premises that did NOT survive verification

- *"The `#2923` FxHash guard's textual needle is narrower than the codebase's
  house style, mirroring the save audit's `FORMAT_MAJOR` finding."* — The guard
  in `crates/renderer/src/vulkan/context/mod.rs:4347-4368` pins the
  fully-qualified `rustc_hash::FxHashSet<EntityId>` while the house style is
  import-then-bare. But the failure direction is **safe**: a house-style refactor
  breaks the *positive* assertion loudly, it does not silently pass. Disproved,
  not reported. (The genuine gap it exposed — an unguarded fourth collection —
  is TD9-2026-08-16-03.)
- *"`docs/feature-matrix.md`'s M47/M45 rows still lag the code."* — Both were
  corrected on 2026-06-21 and remain accurate. The matrix's live rot is
  elsewhere (TD3-2026-08-16-01).
- *"GPU `#[repr(C)]` size doc comments have drifted again."* — `GpuInstance`
  128 B, `GpuCamera` 336 B, `GpuMaterial` 348 B, `GpuTerrainTile` 96 B and
  `Vertex` 104 B are consistent across every live doc comment and pinned test.
  The 100-byte `Vertex` sites found on 2026-08-12 are fixed. Only historical
  audit reports carry superseded values, which is correct for an archive.

---

## Baseline Snapshot (for the next audit's diff)

```
TODO/FIXME/HACK/XXX:      20   (0 real — all protocol / upstream-ref / prose)
allow(dead_code):         58   (24 of them one ALIAS_FLAG_* block in quest.rs)
unimplemented!/todo!():    0
#[ignore] tests:         140   (crates + byroredux + tools; all data/GPU gated)
files >2000 LOC:          11   (SKILL.md orientation says 6)
  ...of which majority-test:  7
  ...of which pure test file: 2
GLSL shaders:             21   (17 include shader_constants.glsl; guard lists 16)
```

Command set used is unchanged from the SKILL's Phase 1 except the `#[ignore]`
count, which was scoped to `crates byroredux tools` rather than `.` (see #2262).

---

## Top Quick Wins (trivial, ≤30 min each)

1. **TD9-2026-08-16-02** — add `presentation.frag` to
   `affected_shaders_include_constants_header`'s list. One line.
2. **TD3-2026-08-16-02** — delete the "no consumer exists yet" sentence from
   `crates/core/src/combat.rs`'s module docs.
3. **TD3-2026-08-16-03** — correct `byroredux/src/combat.rs:159-163`'s comment:
   the consumer *recomputes* the damage, it does not read the trace.
4. **TD8-2026-08-16-02** — drop the redundant
   `#[cfg_attr(not(test), allow(dead_code))]` on `ActionState::is_held`.
5. **TD4-2026-08-16-02** — rename the three `AUDIT_TECH-DEBT_*.md` files (and
   the two `AUDIT_LEGACY-COMPAT_*.md` siblings) to the underscore form.
6. **TD8-2026-08-16-01** — collapse 25 per-item `#[allow(dead_code)]` into one
   module-level attribute, or re-export the 20 unreachable constants.
7. **TD3-2026-08-16-01** — replace the `docs/feature-matrix.md` "Native menu
   reimplementation | Not planned" row and refresh the gap-section date.

## Top Medium Investments

1. **TD4-2026-08-16-01** — change Dimension 1's discovery recipe from total LOC
   to production LOC (`awk` split at the first `#[cfg(test)]`), and add a
   separate "test file >2000 LOC" bucket. This is what makes the next five
   Dim-1 findings real instead of noise.
2. **TD6-2026-08-16-01** — either wire `CombatState.blocking` into
   `HitEvent.blocked` at the producer, or remove the `Block` bindings and the
   dead consumer arm. Shipping a bound action that provably does nothing is the
   worse of the two states.
3. **TD9-2026-08-16-01** — give the `ALIAS_FLAG_*` catalog the same
   declaration-count parity check `dbg_bits_catalog_covers_every_dbg_constant`
   already applies to `DBG_*`, and replace the tautological
   `combined.has(flag)` loop with a single-bit / value assertion.
4. **TD2-2026-08-16-01** — drive the GLSL raw-debug predicate from `DBG_BITS`
   (or a generated `#define`) instead of hand-writing it in two shaders and
   pinning it with four string literals.
5. **TD1-2026-08-16-01** — split `crates/renderer/src/vulkan/acceleration/tests.rs`
   per the production module it exercises, mirroring the Session-35 split of the
   production side it was created by.

---

# Findings

## MEDIUM

### TD4-2026-08-16-01: Dimension 1's discovery recipe measures total file LOC — a proxy — and 7 of the 11 files it flags today are majority-test

- **Severity**: MEDIUM
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:70-72` (Phase-1 snapshot), `:99-103` (Dim 1 Discovery)
- **Status**: NEW
- **Effort**: small
- **Description**: Dimension 1 exists to find *production* complexity —
  "an oversized file taxes every edit, review, and merge", with split axes
  proposed "by responsibility". Its discovery command is
  `find … -exec wc -l {} + | awk '$1>2000'`, which measures total file length,
  test modules included. On the current tree that proxy and the property of
  interest have decoupled: **7 of the 11 files it reports have production halves
  under 1300 LOC**, and **2 are pure test files with no production code at all**.
  The recipe therefore generates findings that the next auditor must
  individually disprove, and it has already done so — the title of the issue it
  produced for `material.rs`, #2257, is literally *"crossed 2000 LOC — mostly
  inline test growth, **no oversized production function**"*. An issue whose own
  title says it is not the debt the dimension hunts is the recipe reporting a
  proxy.
- **Evidence**: live set with the production/test split (line number of the
  first `#[cfg(test)]`):

  | File | Total | Production | Status |
  |---|---|---|---|
  | `crates/renderer/src/vulkan/context/draw.rs` | 4594 | 4594 | real |
  | `crates/renderer/src/vulkan/context/mod.rs` | 4433 | ~4330 | real (#1749) |
  | `crates/renderer/src/vulkan/acceleration/tests.rs` | 2327 | **0** | pure test |
  | `crates/renderer/src/vulkan/svgf.rs` | 2238 | 1700 | majority-test |
  | `crates/renderer/src/vulkan/volumetrics.rs` | 2212 | — | #2256 OPEN |
  | `crates/renderer/src/vulkan/material.rs` | 2173 | — | #2257 OPEN |
  | `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` | 2125 | **0** | pure test |
  | `crates/plugin/src/esm/records/misc/world.rs` | 2116 | 1202 | majority-test |
  | `crates/physics/src/world.rs` | 2089 | 1017 | majority-test |
  | `byroredux/src/env_translate.rs` | 2081 | 869 | majority-test |
  | `crates/renderer/src/texture_registry.rs` | 2021 | 838 | majority-test |

  The SKILL's own orientation paragraph names a 6-file set of which two
  (*`byroredux/src/save_io.rs`*, *`byroredux/src/asset_provider/tests/`*) have
  since fallen back under the threshold and seven others have crossed — so the
  named composition is wrong in both directions, not merely numerically stale.
- **Impact**: Every tech-debt run pays triage cost on ~7 false-positive
  candidates, and the two genuinely oversized production files
  (`context/draw.rs`, `context/mod.rs`) are buried in that noise. A dimension
  that mostly reports its own measurement artifact stops being read.
- **Related**: #2257, #2256 (both OPEN, both produced by this recipe); #2262
  (the same class of recipe defect in the `#[ignore]` count).
- **Suggested Fix**: Split the recipe in two — production LOC (`awk` truncating
  at the first `#[cfg(test)]` / `mod tests`) against the 2000 threshold, and a
  separate, lower-priority "test file >2000 LOC" bucket. Update the orientation
  paragraph to quote the production figure.

---

### TD4-2026-08-16-02: Three tech-debt reports are invisible to Phase 1's own dedup scan

- **Severity**: MEDIUM
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:64`, `docs/audits/`
- **Status**: NEW (consequence not stated by #2422)
- **Effort**: trivial
- **Description**: Phase 1 step 4 says *"Scan `docs/audits/` for prior
  `AUDIT_TECH_DEBT_*.md`"*. That glob matches 19 files. Three further tech-debt
  reports exist under a **hyphen** separator — `AUDIT_TECH-DEBT_2026-07-16.md`,
  `AUDIT_TECH-DEBT_2026-08-03.md`, `AUDIT_TECH-DEBT_2026-08-07.md` — and are
  never read. Between them they carry 40+ findings including a full 9-dimension
  sweep (2026-08-07), the `TD7-101` shader render-layer finding, and the
  `TD3-204`/`TD3-208` GPU-size doc-rot family. Dedup is declared MANDATORY, so a
  scan that structurally cannot see 14 % of the prior corpus is a
  green-by-construction dedup step: it reports "no prior coverage" for subjects
  that were audited three times.
- **Evidence**:
  ```
  $ ls docs/audits/ | grep -E 'AUDIT_[A-Z]+-[A-Z]'
  AUDIT_LEGACY-COMPAT_2026-07-02.md
  AUDIT_LEGACY-COMPAT_2026-07-16.md
  AUDIT_TECH-DEBT_2026-07-16.md
  AUDIT_TECH-DEBT_2026-08-03.md
  AUDIT_TECH-DEBT_2026-08-07.md
  ```
  This run only found them by grepping `*.md` rather than the specified glob.
  #2422 (OPEN) reports the *convention* violation and states "Two audit report
  filenames"; the live count is **five across two audit types**, and neither the
  issue nor the SKILL states the dedup consequence.
- **Impact**: Silent under-dedup on every future tech-debt and legacy-compat
  audit — the exact failure mode `_audit-common.md`'s dedup section exists to
  prevent. Also inflates the apparent novelty of re-found findings.
- **Related**: #2422 (OPEN — filename convention, understates the count and does
  not name the dedup consequence).
- **Suggested Fix**: `git mv` the five files to the underscore form, and make
  Phase 1 step 4 glob `AUDIT_TECH[-_]DEBT_*.md` so a future slip degrades
  gracefully rather than silently.

---

### TD3-2026-08-16-01: `docs/feature-matrix.md` states native menus are "Not planned" while the engine ships a three-page native game menu

- **Severity**: MEDIUM
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:207`, `:224`
- **Status**: NEW
- **Age**: row written `58e14e04`, 2026-07-25; falsified 2026-08-15/16
- **Effort**: trivial
- **Description**: The UI table's last row reads
  `| Native menu reimplementation | Not planned; preserve SWF compatibility through Ruffle profiles |`.
  That is a **policy statement**, and it is now false. `crates/debug-ui` ships
  `GameMenuPage::{Pause, Settings, Inventory}` with `draw_game_menu`,
  `byroredux/src/inventory.rs` (546 LOC, "Native inventory presentation and
  player-facing equipment mutations") supplies its `InventorySnapshot` and
  consumes its `InventoryAction`, and `byroredux/src/settings_io.rs` (334 LOC,
  "Persistent user settings for the native menu") persists the Settings page.
  This is doc rot in the direction that costs most: documented capability is
  *lower* than reality, so a reader — including the next auditor — concludes
  that ~900 LOC of shipped, scheduled code should not exist. The same section is
  headed *"What Doesn't Work Yet (live gaps as of 2026-08-12)"* and has no
  gameplay/combat rows at all, four days after the P2 melee core landed.
- **Evidence**:
  - `docs/feature-matrix.md:207` — the row above.
  - `crates/debug-ui/src/panels.rs:185-190` — `enum GameMenuPage { Pause, Settings, Inventory }`.
  - `crates/debug-ui/src/lib.rs:279-289` — `open_inventory` / `close_game_menu`; `:348` — `panels::draw_game_menu`.
  - `byroredux/src/inventory.rs:255` — `snapshot(world) -> Option<byroredux_debug_ui::InventorySnapshot>`; `:319` — `apply_action(world, InventoryAction)`.
  - `byroredux/src/settings_io.rs:1-7` — module docstring.
  - `ROADMAP.md:562-599` documents the whole P0/P1/P2 slice and is current to 2026-08-16, so this is a matrix-only lag, not a project-wide one.
- **Impact**: The matrix is named in `_audit-common.md` as one of eight
  authoritative reference docs ("prefer them over re-deriving facts from
  source"). An auditor who obeys that instruction is told the native menu is out
  of scope by policy. The UI section also mis-frames `/audit-ui`'s subject area.
- **Related**: #2961 (OPEN — the same file has no character/progression rows;
  sibling gap, different subject); #2729 (OPEN — ROADMAP M48 input-routing row).
- **Suggested Fix**: Replace the row with the real state (native egui game menu
  with Pause/Settings/Inventory pages shipped 2026-08-15/16; Scaleform remains
  the compatibility target for authored Bethesda menus), add a
  Gameplay/Combat section covering the P0–P2 slice, and refresh the gap-section
  date.

---

### TD6-2026-08-16-01: `InputAction::Block` is bound to two inputs and a console command but has no gameplay effect; its consumer arm is unreachable

- **Severity**: MEDIUM
- **Dimension**: 6 — Stub & Placeholder Implementations
- **Location**: `byroredux/src/combat.rs:46`, `:74-81`, `:168`, `:203-207`; `byroredux/src/interaction.rs:141`, `:146`, `:542`
- **Status**: NEW
- **Age**: `4a404f5c` / `eb5d76fe`, 2026-08-15/16
- **Effort**: small
- **Description**: `Block` is a fully-wired input action — `KeyCode::KeyC`,
  `MouseButton::Right`, and the console tokens `"block" | "c"` reachable through
  `input.press` / `input.hold`. `combat_input_system` reads it into
  `CombatState.blocking`. Nothing else reads `blocking` except the
  `combat.status` display string. Meanwhile the *sole* `HitEvent` producer in
  the workspace hardcodes `blocked: false`, so `combat_damage_system`'s
  zero-damage arm is unreachable from any live path. Blocking therefore costs
  the player nothing and gains them nothing; the only observable is a debug
  string. This is a stub reachable from a shipped console command, which the
  severity table promotes to MEDIUM.
- **Evidence**: `combat.rs:74-81`
  ```rust
  actions.is_held(InputAction::Block),
  …
  state.blocking = block_held;
  ```
  `combat.rs:157-169` — the only `HitEvent` construction outside tests:
  ```rust
  byroredux_scripting::HitEvent {
      aggressor, source: aggressor, projectile: 0,
      power_attack: false, sneak_attack: false,
      bash_attack: false, blocked: false,
  }
  ```
  `combat.rs:203-207` — the arm that can never be taken:
  ```rust
  let damage = if event.blocked { 0.0 } else { attack_damage(world, event.aggressor) };
  ```
  Workspace grep confirms `combat.rs` is the only non-test `HitEvent` producer
  (the other hits are the struct definition, `register`, the Late-stage drain,
  and a recognizer-table doc comment). The only reader of `blocking` is
  `byroredux/src/commands/view.rs:102-104`.
  Four sibling `HitEvent` fields — `projectile`, `power_attack`, `sneak_attack`,
  `bash_attack` — are likewise constant at the producer with no reader anywhere.
- **Impact**: A player (or the p2 smoke gate, or a future combat test) who holds
  Block takes full damage. Because `CombatState.blocking` *is* surfaced by
  `combat.status`, the console reports a defensive state the damage pipeline
  does not honour — which is worse than reporting nothing, since it reads as
  working. The unreachable arm also means any future `blocked`-aware regression
  test is green by construction until a producer sets the flag.
- **Related**: AUDIT_ECS_2026-08-16 ECS-2026-08-16-04 (the parallel
  `EquippedWeapon` write-path gap in the same slice); CHAR-2026-08-16-D1-01
  (`attack_damage` bypasses CHARAL entirely).
- **Suggested Fix**: Set `blocked: state.blocking` at the producer and pin it
  with a `combat_damage_system` unit test asserting a blocked hit applies zero
  damage and still counts as a hit — or, if damage mitigation is deferred,
  delete the `Block` bindings and the consumer arm and say so in
  `docs/engine/playable-vertical-slice.md` rather than shipping an inert
  binding.

---

## LOW

### TD1-2026-08-16-01: The >2000-LOC set grew 6 → 11; seven newly-crossed files are unfiled and two are pure test files

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: see table in TD4-2026-08-16-01
- **Status**: NEW (for the seven; `volumetrics.rs`/`material.rs` are #2256/#2257, `context/mod.rs` is #1749)
- **Effort**: medium (per file)
- **Description**: Seven files crossed 2000 LOC since the last recorded set and
  none has an issue. Read against TD4-2026-08-16-01, only two of them are
  production complexity worth acting on; the rest are test growth. Recording the
  membership change here so the next audit can diff, and proposing split axes
  only where the axis is real:
  - **`crates/renderer/src/vulkan/acceleration/tests.rs` (2327, pure test)** —
    created by the Session-35 split of `acceleration.rs` into
    `{constants, types, predicates, blas_static, blas_skinned, tlas, memory}`,
    but the tests were *not* split with it. Split axis: mirror the production
    module names it already exercises (`blas_static_tests.rs`,
    `blas_skinned_tests.rs`, `tlas_tests.rs`, `memory_tests.rs`,
    `predicates_tests.rs`), which is the pattern every other refactored module
    in the tree already follows (`crates/nif/src/import/tests/`,
    `crates/plugin/src/esm/cell/tests/`, `byroredux/src/render/*_tests.rs`).
  - **`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`
    (2125, pure test)** — same treatment; it now holds shader-source contract
    tests (`correctness_debug_views_bypass_non_transport_frame_graph_terms`,
    `gi_zero_budget_is_a_true_no_ray_floor`,
    `normal_alpha_masks_specular_intensity_not_roughness`) that are not layout
    tests at all and belong beside the shader-constants suite.
  - `svgf.rs` (1700 prod), `esm/records/misc/world.rs` (1202 prod),
    `crates/physics/src/world.rs` (1017 prod), `env_translate.rs` (869 prod),
    `texture_registry.rs` (838 prod) — **no production split warranted**;
    recorded for the diff only.
  Two files present in the SKILL's orientation set have fallen back under
  threshold: *`byroredux/src/save_io.rs`* and *`byroredux/src/asset_provider/tests/`*.
- **Evidence**: `find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1>2000'`;
  production halves measured at the first `#[cfg(test)]`.
- **Impact**: Low on its own. The signal is that the *production* side of the
  set has been stable at two files for several sweeps while the test side grew
  by five — worth knowing before anyone budgets a "split the big files" pass.
- **Related**: TD4-2026-08-16-01; #2256, #2257, #1749.
- **Suggested Fix**: File the two test-file splits; leave the five majority-test
  production files alone.

---

### TD2-2026-08-16-01: The raw-debug-output predicate is hand-written in Rust and twice in GLSL, and its guard is a four-literal subset check

- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `crates/renderer/src/shader_constants.rs:33-41`; `crates/renderer/shaders/presentation.frag:136-139`; `crates/renderer/shaders/composite.frag`; guard at `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:695-710`
- **Status**: NEW
- **Effort**: small
- **Description**: "Which debug visualizations are correctness oracles that must
  bypass the post-transport frame graph" is one policy expressed at three sites
  in two languages. Rust:
  ```rust
  pub const fn debug_viz_requires_raw_output(flags: u32) -> bool {
      flags & (DBG_VIZ_SELECTED_LIGHT | DBG_VIZ_DIRECT | DBG_VIZ_RAW_INDIRECT) != 0
          || (flags & DBG_VIZ_RT_LOD) == DBG_VIZ_RT_LOD
  }
  ```
  GLSL (`presentation.frag:136-139`, and the same four clauses in
  `composite.frag`):
  ```glsl
  bool rawDebug = (dbgFlags & DBG_VIZ_SELECTED_LIGHT) != 0u
      || (dbgFlags & DBG_VIZ_DIRECT) != 0u
      || (dbgFlags & DBG_VIZ_RAW_INDIRECT) != 0u
      || (dbgFlags & DBG_VIZ_RT_LOD) == DBG_VIZ_RT_LOD;
  ```
  The guard that is supposed to hold them in lockstep asserts only that each of
  four hardcoded strings is *present* in each shader. It is a subset check
  against an expected set derived from nothing, so adding a fifth raw-output
  view to the Rust function — the natural next edit, since the sibling test
  `correctness_debug_views_require_raw_frame_graph_output` already lists **six**
  view constants — leaves both shaders silently tone-mapping a correctness
  oracle while the whole suite stays green.
- **Evidence**: `shader_constants.rs:93-100` asserts the Rust predicate over six
  inputs (`SELECTED_LIGHT`, `SHADOW_VISIBILITY`, `MATERIAL_LOBES`, `RT_LOD`,
  `DIRECT`, `RAW_INDIRECT`); `gpu_instance_layout_tests.rs:698-706` pins the
  GLSL side with four `source.contains(…)` literals. Nothing relates the two
  sets. The house pattern for exactly this problem exists two files away —
  `generated_header_contains_all_defines` iterates the shared `DBG_BITS` catalog
  precisely "so this value-pin can never again cover a subset (#1482 / #1860)".
- **Impact**: A debug view whose entire purpose is to be an unmodified oracle
  gets ACES tone-mapping, exposure and grading applied on the presentation pass,
  making black-vs-dim and isolated-energy readings meaningless — with no test
  failure. Blast radius is developer tooling only, not shipped rendering.
- **Related**: #2800, #2799, #2798 (the same "shader doc/guard describes a
  different thing than the code" family in the renderer).
- **Suggested Fix**: Emit a `DBG_VIZ_RAW_OUTPUT_MASK` (and the `RT_LOD` compound
  test) from `shader_constants_data.rs` and have both shaders consume it, so the
  policy lives in one place and `generated_header_contains_all_defines` covers
  it for free.

---

### TD3-2026-08-16-02: `crates/core/src/combat.rs` still asserts no combat consumer exists in the engine — one shipped a day earlier

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/core/src/combat.rs:10-11`
- **Status**: NEW
- **Age**: docstring `9cf93368`, 2026-07-04; falsified `eb5d76fe`, 2026-08-16
- **Effort**: trivial
- **Description**: The module docstring reads *"No combat/attack-resolution
  consumer system exists yet in the engine; this module is the reusable, tested
  piece built ahead of that consumer."* `byroredux/src/combat.rs` +
  `combat_input_system` + `combat_damage_system` — a complete
  ray → hit → damage → death pipeline registered as two `Stage::Update`
  exclusives in `byroredux/src/boot.rs:780-781` — is exactly that consumer, and
  landed 2026-08-15/16. It simply took a different path. The docstring's claim
  is the *reason* the module's zero-caller state reads as deliberate rather than
  as a gap, so leaving it in place converts a real design question into a
  documented non-issue.
- **Evidence**: `crates/core/src/combat.rs:10-11` (quoted above). Workspace grep
  for `modified_skill` / `oblivion_weapon_damage_multiplier` /
  `oblivion_hand_to_hand_damage` / `byroredux_core::combat` outside the file
  itself returns **zero** hits. `byroredux/src/combat.rs` does not import
  `byroredux_core::character` or `byroredux_core::combat` at all; its damage
  model is `EquippedWeapon.damage` or the flat `UNARMED_DAMAGE = 8.0`.
  `crates/core/src/stealth.rs` (487 LOC) is in the same state, and its
  `sneak_attack` counterpart is hardcoded `false` at the HitEvent producer.
- **Impact**: Doc rot only — the correctness half is already owned by
  CHAR-2026-08-16-D1-01. But this is the sentence that will keep the next reader
  from asking why two combat-math modules exist unconnected beside a third that
  is connected and uses neither.
- **Related**: CHAR-2026-08-16-D1-01 (same sweep — the consumer bypasses CHARAL;
  names the zero-caller state but not this docstring); #2962 (OPEN — ownership
  of `crates/core/src/combat.rs` and `stealth.rs`).
- **Suggested Fix**: Reword to name the live consumer and state plainly that it
  does not route through this module yet, with a pointer to #2962 /
  CHAR-2026-08-16-D1-01. Same for `crates/core/src/stealth.rs` if it carries an
  equivalent claim.

---

### TD3-2026-08-16-03: `combat_input_system`'s comment says the damage is re-read from the trace; the consumer recomputes it

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `byroredux/src/combat.rs:159-163`, against `:203-207` and `:269-273`
- **Status**: NEW
- **Age**: `eb5d76fe`, 2026-08-16
- **Effort**: trivial
- **Description**: The comment justifying `source: aggressor` ends *"damage was
  snapshotted into the trace and is re-read same-frame by the consumer."* No
  such read happens. `combat_input_system` computes `damage` at `:150`, stores
  it in `CombatTraceEntry.damage`, and `combat_damage_system` then calls
  `attack_damage(world, event.aggressor)` again at `:206` and overwrites the
  trace entry wholesale at `:245-252`. The value is computed twice from the same
  source and the first copy is discarded. Harmless today because both calls run
  in the same frame against an unchanged `EquippedWeapon` — but the comment
  documents a data-flow contract the code does not implement, which is precisely
  what makes a future `EquippedWeapon` writer (ECS-2026-08-16-04's suggested fix)
  a silent divergence rather than a caught one.
- **Evidence**:
  ```rust
  // combat.rs:159-163
  // Equipped weapons are inventory records rather than standalone
  // ECS entities today. Use the aggressor as the source until item
  // instances acquire stable entities; damage was snapshotted into
  // the trace and is re-read same-frame by the consumer.
  source: aggressor,
  ```
  ```rust
  // combat.rs:203-207 — the "consumer"
  let damage = if event.blocked { 0.0 } else { attack_damage(world, event.aggressor) };
  ```
  `CombatState.last.damage` has exactly one reader: `commands/view.rs`'s
  `combat.status` formatter.
- **Impact**: Documentation only, but it is load-bearing for the next edit to
  this file — the two most likely near-term changes (a runtime `EquippedWeapon`
  writer, and routing damage through CHARAL) both turn "computed twice" into a
  correctness question.
- **Related**: TD6-2026-08-16-01; ECS-2026-08-16-04; CHAR-2026-08-16-D1-01.
- **Suggested Fix**: Either add a `damage: f32` field to `HitEvent` and have the
  consumer read it (which also fixes the scripted-producer case), or correct the
  comment to say the consumer recomputes.

---

### TD8-2026-08-16-01: 20 of 25 `ALIAS_FLAG_*` constants are unreachable outside their own test module; the 5 that are reachable carry a redundant `allow(dead_code)`

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `crates/plugin/src/esm/records/misc/quest.rs:220-286`; re-export at `crates/plugin/src/esm/records/misc.rs:73-77`
- **Status**: NEW
- **Age**: `a844c26b`, 2026-08-07
- **Effort**: trivial
- **Description**: `mod quest;` is private (`misc.rs:39`) and its `pub use`
  block re-exports exactly five of the twenty-five `ALIAS_FLAG_*` constants —
  `RESERVES`, `ALLOW_REUSE`, `ALLOW_DEAD`, `ALLOW_RESERVED`, `CLOSEST`, which
  are the five `crates/scripting/src/scene/quest_alias.rs` consumes. The other
  **twenty** are not re-exported, so no code outside `quest.rs` can name them;
  their only use in the whole tree is the `ALL_FLAGS` array in that file's own
  test module. Symmetrically, the five that *are* re-exported are reachable
  through a `pub use` chain and therefore cannot trip the `dead_code` lint at
  all — their `#[allow(dead_code)]` is inert. So the block carries 25
  attributes of which 5 do nothing and 20 mark genuinely unreachable data.
- **Evidence**:
  ```
  $ grep -c 'pub const ALIAS_FLAG_' crates/plugin/src/esm/records/misc/quest.rs
  25
  $ grep -o 'ALIAS_FLAG_[A-Z_]*' crates/plugin/src/esm/records/misc.rs | sort -u | wc -l
  5
  $ grep -rn 'ALIAS_FLAG_' crates byroredux | grep -v 'records/misc/quest.rs'
  # → only misc.rs / records/mod.rs re-exports and scripting/scene/quest_alias{,_tests}.rs
  ```
  The block comment claims the catalog "stays parser-owned" and "exposes
  remaining authored metadata for later gameplay components" — accurate as
  intent, but no consumer can currently reach that metadata.
- **Impact**: Low. The values are correct-shaped parsed protocol data and
  deleting them would be wrong. The cost is 25 lines of attribute noise and a
  misleading signal that the catalog is available to consumers when 80 % of it
  is not.
- **Related**: TD9-2026-08-16-01 (the guard that is supposed to exercise them);
  #1761 (TD8-004, OPEN — the same "attribute outlived its need" shape in
  `Dx10Chunk::start_mip`).
- **Suggested Fix**: Widen the `pub use` to re-export all twenty-five (they are
  a protocol catalog, and the crate is workspace-internal so there is no API
  surface cost), which removes the need for any `allow` at all. Failing that,
  collapse to one module-level `#![allow(dead_code)]` with the existing comment.

---

### TD8-2026-08-16-02: `ActionState::is_held`'s test-only `allow(dead_code)` is redundant — it has ~20 production callers

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/interaction.rs:582-585`
- **Status**: NEW (regression of the "justified" verdict in AUDIT_TECH_DEBT_2026-08-12 § TD8-2026-08-12-04)
- **Age**: attribute `fe3431e4`, 2026-08-09; invalidated by `4a404f5c`/`eb5d76fe`, 2026-08-15/16
- **Effort**: trivial
- **Description**: `#[cfg_attr(not(test), allow(dead_code))]` on
  `ActionState::is_held` dates from when the action layer was test-only. It now
  has non-test callers in three modules —
  `byroredux/src/systems/character.rs` (6), `byroredux/src/systems/camera.rs`
  (6), and `byroredux/src/combat.rs:75` — so the attribute suppresses nothing.
  The 2026-08-12 sweep examined this cluster and recorded all four attributes as
  justified; one week of gameplay-slice work invalidated one of them, which is
  the point of re-running the recipe.
- **Evidence**:
  ```
  $ grep -rn '\.is_held(' byroredux/src | grep -v '_tests.rs' | grep -v 'assert'
  byroredux/src/combat.rs:75
  byroredux/src/systems/character.rs:168,171,174,177,180,181
  byroredux/src/systems/camera.rs:53,56,59,62,65,73
  ```
  Its sibling `was_released` (`:591-594`) still has zero non-test callers — its
  attribute remains correct, so this is a one-site fix, not a cluster removal.
- **Impact**: None functional. It is a false "this is only used by tests" signal
  on the single most-called accessor in the input layer.
- **Related**: AUDIT_TECH_DEBT_2026-08-12 § TD8-2026-08-12-04.
- **Suggested Fix**: Delete the attribute on `is_held`; leave `was_released` and
  `ActionBindings::bind_key` as they are.

---

### TD9-2026-08-16-01: `alias_flags_has_recognizes_every_named_bit` is tautological in its main loop and its roster is hand-maintained with no parity check

- **Severity**: LOW
- **Dimension**: 9 — Test Hygiene (green-by-construction)
- **Location**: `crates/plugin/src/esm/records/misc/quest.rs:1905-1954`, against `crates/plugin/src/esm/records/misc/quest.rs:226-232`
- **Status**: NEW
- **Effort**: trivial
- **Description**: The test's doc comment states its purpose: *"guards against a
  copy-paste bit-value typo in the catalog (each must be its own distinct,
  correctly-shifted bit)"*. Two of its three assertions do real work; the
  headline one cannot fail:
  ```rust
  let combined = AliasFlags(ALL_FLAGS.iter().fold(0u32, |acc, &f| acc | f));
  for &flag in ALL_FLAGS {
      assert!(combined.has(flag), "bit {flag:#x} not set in the combined mask");
  }
  ```
  `(a | b | c) & a != 0` is true for every non-zero `a` **by construction** — the
  loop is an identity over the OR-fold that produced it. It can only fail if a
  constant is literally `0`.
  The `sorted.dedup()` length check does catch two constants sharing a value,
  and `!combined.has(0x8000_0000)` catches a bit-31 constant. Neither catches a
  constant that is a *wrong but distinct* bit (the exact defect the doc names),
  nor a multi-bit value, which the "correctly-shifted" claim asserts against.
  The deeper issue is the roster: `ALL_FLAGS` is a hand-copied list of the 25
  constants with **no parity check against the declarations**. A 26th constant
  added tomorrow is never exercised and the test stays green — while the
  `#[allow(dead_code)]` block comment at `:226-232` asserts *"Every constant is
  exercised by an `AliasFlags::has` assertion in the test module below"*, a
  claim that silently becomes false. The codebase already solved this exact
  problem: `dbg_bits_catalog_covers_every_dbg_constant`
  (`crates/renderer/src/shader_constants.rs:62-80`) counts
  `pub const DBG_` occurrences in the source text specifically so the catalog
  "cannot silently drift behind a new constant again". That pattern was not
  applied here.
- **Evidence**: quoted above; the parity-check counterexample is
  `shader_constants.rs:62-80`. No wire-level test anywhere decodes a real `FNAM`
  payload and asserts a named alias flag, so the 25 values have no external
  authority behind them either — deliberately not claimed as wrong here (no
  guessing), only as unverified.
- **Impact**: A test that reads as a value guard and is a presence guard. Cost
  today is confidence, not behaviour: five of the constants drive live alias-fill
  policy in `crates/scripting/src/scene/quest_alias.rs:487-568` (dead-actor
  eligibility, reservation reuse, closest-match), so a wrong-but-distinct bit
  there silently changes which references fill a quest alias.
- **Related**: TD8-2026-08-16-01 (the same block); #1482 / #1860 (the two prior
  rounds of exactly this defect on the `DBG_*` catalog).
- **Suggested Fix**: Add the declaration-count parity assertion (copy the
  `dbg_bits_catalog_covers_every_dbg_constant` shape), and replace the
  tautological loop with `assert_eq!(flag.count_ones(), 1)` plus an explicit
  `assert_eq!(ALIAS_FLAG_X, 0x…)` value pin per constant.

---

### TD9-2026-08-16-02: The shader-include allow-list covers 16 of the 17 live header consumers — `presentation.frag` is missing

- **Severity**: LOW
- **Dimension**: 9 — Test Hygiene (green-by-construction)
- **Location**: `crates/renderer/src/shader_constants.rs:324-394`
- **Status**: NEW
- **Age**: `presentation.frag` gained the include in `5f970bae`, 2026-08-15
- **Effort**: trivial
- **Description**: `affected_shaders_include_constants_header` exists because
  "a shader that drops the `#include` would otherwise compile against undefined
  identifiers … and no `cargo test` would catch it (the SPIR-V is
  pre-compiled)". Its own doc comment states the invariant it must satisfy:
  *"this allow-list MUST cover every shader that consumes a generated macro from
  `shader_constants.glsl`"*, with an explicit maintenance instruction —
  *"Cross-check when adding a shader: `grep -L` the include across
  `shaders/*.{comp,frag,vert}` and reconcile against this list."* That
  reconciliation has not happened. The list holds 16 entries;
  `crates/renderer/shaders/presentation.frag` includes the header at line 4 and
  consumes four generated macros (`DBG_VIZ_SELECTED_LIGHT`, `DBG_VIZ_DIRECT`,
  `DBG_VIZ_RAW_INDIRECT`, `DBG_VIZ_RT_LOD`) at `:136-139`. It is the only
  omission — the list is otherwise exactly right.
- **Evidence**:
  ```
  $ cd crates/renderer/shaders && grep -l 'include/shader_constants.glsl' *.vert *.frag *.comp | wc -l
  17
  ```
  Live consumers: `bloom_downsample.comp`, `bloom_upsample.comp`,
  `caustic_splat.comp`, `cluster_cull.comp`, `composite.frag`,
  **`presentation.frag`**, `skin_palette.comp`, `skin_vertices.comp`,
  `ssao.comp`, `svgf_atrous.comp`, `svgf_temporal.comp`, `taa.comp`,
  `triangle.frag`, `triangle.vert`, `volumetrics_inject.comp`,
  `volumetrics_integrate.comp`, `water.frag`. The test's array
  (`shader_constants.rs:340-388`) lists all but the bolded one.
  This is the same defect the list was *last* expanded for: #1780 added six
  previously-unlisted header consumers.
- **Impact**: Removing `presentation.frag`'s `#include` — plausible during a
  post-pass refactor, since the shader's only generated-macro use is one debug
  branch — would leave `DBG_VIZ_*` undefined at recompile time with no test
  failure. The presentation pass is the engine-default output stage since FSR
  phase 7, so the recompile break lands on the final swapchain write.
- **Related**: #1780 (the previous round of the same omission).
- **Suggested Fix**: Add the entry. Better: replace the hand-maintained array
  with a compile-time enumeration in `build.rs` (which already walks
  `shaders/`), so the list cannot lag the directory a third time.

---

### TD9-2026-08-16-03: `skin_offsets` — one of the four collections the #2923 hot-path rule names — has no hasher guard

- **Severity**: LOW
- **Dimension**: 9 — Test Hygiene (green-by-construction)
- **Location**: `byroredux/src/main.rs:158`, `byroredux/src/render/static_meshes.rs:57`, `byroredux/src/app_frame.rs:138`; guards at `crates/core/src/ecs/resources/skin_slot_pool.rs:899-963` and `crates/renderer/src/vulkan/context/mod.rs:4338-4368`
- **Status**: NEW
- **Effort**: trivial
- **Description**: `_audit-common.md`'s hot-path hashing rule names four things
  that must stay `Fx`-hashed across the crate boundary: every `SkinSlotPool`
  collection, the `pose_dirty` set it hands the renderer, `FrameInputs.pose_dirty`,
  and *"the `skin_offsets` map threaded through `byroredux/src/render/`"*. The
  #2923 fix shipped two source-text guards covering the first three —
  `skin_slot_pool_maps_are_not_siphash` (five fields),
  `pose_dirty_accessor_does_not_pin_siphash_across_the_crate_boundary`, and
  `pose_dirty_crosses_the_crate_boundary_without_siphash` (`draw.rs` +
  `skinned_blas_refit.rs`). Nothing pins `skin_offsets`. It is `FxHashMap` today
  at all three sites, so this is a coverage gap, not a live regression — but the
  guards' own doc comment records that this defect class has already recurred
  three times at three different sites (#1368 → #2174 → #2923, each sweep
  "missing this cluster entirely"), which is exactly the argument for pinning
  the fourth.
- **Evidence**: `byroredux/src/main.rs:158`
  ```rust
  skin_offsets: rustc_hash::FxHashMap<byroredux_core::ecs::EntityId, u32>,
  ```
  `byroredux/src/render/static_meshes.rs:57` takes
  `skin_offsets: &FxHashMap<EntityId, u32>` and probes it once per draw at
  `:253` (`skin_offsets.get(&entity)`), inside the static-mesh main loop —
  per-frame, per-entity, the same access shape that made `pose_dirty` worth
  guarding. `grep -rn 2923 byroredux/src` returns nothing.
- **Impact**: None today. The gap is that the one collection in the rule with no
  guard is the one in the binary crate, which is where the previous two
  regressions were reintroduced.
- **Related**: #1368, #2174, #2923; `feedback_no_guessing`-adjacent hot-path rule
  in `_audit-common.md:234-246`.
- **Suggested Fix**: Extend `pose_dirty_crosses_the_crate_boundary_without_siphash`'s
  loop (or add a sibling in `byroredux/src/render/`) to include
  `include_str!("../main.rs")` and `include_str!("static_meshes.rs")` with the
  `FxHashMap<EntityId, u32>` needle.

---

## Verified Clean

Dimensions and sub-checks examined that produced no finding, recorded so the
next sweep does not re-derive them:

- **Dim 5 — Stale Markers: 0 findings.** All 20 `TODO|FIXME|HACK|XXX` hits are
  false positives on the documented exclusion classes: the ESM `XXXX`
  extended-size sub-record tag (`crates/plugin/src/esm/reader.rs` ×7,
  `records/misc/magic.rs` ×3, `esm/cell/wrld.rs`, `records/misc/world.rs`),
  documentation *of an upstream reference implementation's* FIXME
  (`crates/bgsm/src/bgem.rs:137`, `crates/nif/src/blocks/bs_geometry.rs:596`,
  `records/misc/world.rs:216`), and prose referring to a closed TODO
  (`byroredux/src/scene.rs:1356`, `byroredux/src/groundcover_translate.rs:252`).
  `crates/renderer/shaders/` has **zero** marker hits. The MIT attribution block
  atop `triangle.frag` is intact.
- **Dim 7 — Magic Numbers: 0 findings.** Every `#define` in
  `crates/renderer/shaders/` outside three `#ifndef` include guards
  (`ray_origin.glsl`, `shadow_transport.glsl`, `shadow_common.glsl`) is
  generated from `shader_constants_data.rs` — the provenance rule holds
  end-to-end. GPU `#[repr(C)]` size literals are pinned and consistent
  (`gpu_instance_is_128_bytes_std430_compatible`, `gpu_camera_is_336_bytes`,
  `gpu_material_size_is_348_bytes`, `gpu_terrain_tile_is_96_bytes`).
- **Dim 3 structural half**: `.claude/commands/_audit-validate.sh` passes —
  1411 refs across 30 skill files, **0 stale paths**, 1 advisory symbol
  (`enhancement`, a GitHub label, correctly non-fatal). The TD7-\* stale-path
  family remains closed.
- **`unimplemented!` / `todo!()` / `panic!("not `**: 0 occurrences workspace-wide.
- **`#[ignore]` triage**: all 140 are gated on installed game data, a Vulkan
  device, or a multi-second corpus walk. None guards a closed CRITICAL/HIGH fix.
- **Assertion-free tests**: 67 candidates enumerated by brace-balanced scan.
  Every one is a `#[should_panic]`, a documented "does not overflow / does not
  panic" test where the absence of a panic *is* the stated assertion
  (`list_shape_mutual_cycle_does_not_overflow`,
  `template_chain_breaks_on_cycle_via_depth_cap`), or a thin delegate to an
  asserting helper (`parse_rate_*`, `per_block_baseline_*`,
  `unknown_ceiling_*`). No finding. The weakest is
  `crates/core/src/character/regen.rs:391 tick_system_is_a_noop_without_config`,
  which verifies "does not panic" while its name claims "is a noop" — noted, not
  filed (CHARAL is owned by `/audit-character`, audited today).
- **Exclusion / allowlist files**: exactly one exists workspace-wide —
  `KNOWN_MISSING_ON_DESTROY_TRAIT` (`crates/ui/src/avm2_host.rs:1374`) — already
  reported by AUDIT_UI_2026-08-16 in this sweep.
- **Dead-code breadcrumbs**: no `// removed:`, no `#[deprecated]`, no orphaned
  `_`-prefixed params. The three `fn *_unused*` hits are legitimate names
  (`evict_unused_blas`, and two test names about unused bytes/slots).

---

## Deferred

| Finding | Gating milestone / reason |
|---|---|
| `HitEvent`'s five constant-at-producer fields (`projectile`, `power_attack`, `sneak_attack`, `bash_attack`, `blocked`) | The struct is the Papyrus `OnHit` parity surface; four of the five need the sneak (`crates/core/src/stealth.rs`), projectile and power-attack subsystems that P2 explicitly defers. Only `blocked` is actionable now — filed as TD6-2026-08-16-01. |
| `crates/core/src/stealth.rs` (487 LOC) zero-consumer state | Same shape as TD3-2026-08-16-02 but the ownership question is #2962 (OPEN). Re-file only if #2962 closes without connecting it. |
| Wire-level value verification of the 25 `ALIAS_FLAG_*` constants | Needs an xEdit/`fopdoc` cross-check against a real `FNAM` payload. Not attempted — per the no-guessing rule, TD9-2026-08-16-01 claims only that the guard cannot detect a wrong value, not that any value is wrong. |

---

## Deduplication Record

Baseline: `/tmp/audit/issues.json` (269 OPEN issues), plus all 22 prior
tech-debt reports — the 19 matching `AUDIT_TECH_DEBT_*.md` **and** the 3
hyphenated ones the specified glob misses (see TD4-2026-08-16-02).

**Skipped as already OPEN:**

| Subject | Issue |
|---|---|
| `volumetrics.rs` / `material.rs` crossed 2000 LOC | #2256, #2257 |
| `VulkanContext::new()` 1025-LOC constructor | #1749 |
| Tech-debt SKILL's `#[ignore]` count recipe scans the whole repo | #2262 |
| `XXXX` false-positive exclusion list missing newest sites | #2263 |
| Audit report filenames using a hyphen | #2422 (understates: 5, not 2 — the *dedup consequence* is filed separately as TD4-2026-08-16-02) |
| `_audit-common.md` shader count 21 vs 19 | #2421 |
| Bare `bsver` literals in NIF block gates | #2423, #2424, #2425 |
| Orphaned synchronous NPC-spawn wrappers (`npc_spawn.rs:850`, `:949`) | #2266 |
| `Dx10Chunk::start_mip` redundant `allow(dead_code)` | #1761 |
| `crates/core/src/combat.rs` + `stealth.rs` un-owned | #2962 |
| `docs/feature-matrix.md` has no character/progression rows | #2961 (sibling gap; TD3-2026-08-16-01 is the UI/gameplay row, a different subject) |

**Skipped as covered by a sibling audit in this same sweep:**

| Subject | Owner |
|---|---|
| `combat::disable_actor_ai` duplicates `clear_ambient_behavior` | AUDIT_ECS_2026-08-16 § ECS-2026-08-16-01 |
| Native inventory cannot equip a weapon; `EquippedWeapon` has no runtime writer | AUDIT_ECS_2026-08-16 § ECS-2026-08-16-04 |
| `combat_input_system` burns the attack edge before the `PlayerMode` gate | AUDIT_ECS_2026-08-16 § ECS-2026-08-16-05 |
| `UNARMED_DAMAGE = 8.0` unsourced; `attack_damage` bypasses CHARAL | AUDIT_CHARACTER_2026-08-16 § CHAR-2026-08-16-D1-01 |
| Death-as-removals vs the additive-only save overlay | AUDIT_SAVE_2026-08-16 |
| `KNOWN_MISSING_ON_DESTROY_TRAIT` exclusion list | AUDIT_UI_2026-08-16 |
| FO4 `parse_weap` zero-damage decode reaching the melee slice | AUDIT_LEGACY_COMPAT_2026-08-16 |

**Disproved candidates** (investigated, could not be sustained — see Executive
Summary for detail): the #2923 `FxHashSet` needle's match direction; a supposed
M47/M45 `feature-matrix.md` lag; GPU `#[repr(C)]` size doc drift;
`water.vert`'s `INSTANCE_FLAG_NUS` (a comment, not a hand-declared constant);
`settings_io.rs`'s version check (warn-and-continue is the documented design).

---

## Next Step

```
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-08-16.md
```
