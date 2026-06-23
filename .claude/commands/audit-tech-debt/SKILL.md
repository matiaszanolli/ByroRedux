---
description: "Audit accumulated technical debt — stale markers, dead code, duplication, magic numbers, stub impls, doc rot, oversized files"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Tech-Debt Audit

Audit ByroRedux for accumulated technical debt: code that compiles, passes
tests, and ships, but quietly raises the cost of every future change. The goal
is **not** correctness bugs (other audits own that) — it is decay that crept in
since the last cleanup pass.

**Every dimension below is a DISCOVERY RECIPE, not a finding list.** Instances
churn between audits (markers get deleted, files get split, line numbers drift).
So each dimension hands you a command to enumerate *current* instances, then a
triage rule. Do not trust any hardcoded instance list — there are none here on
purpose. Re-run the recipe; report what it surfaces today.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, the 21-crate roster,
methodology, deduplication, context rules, severity, and finding format. Do not
duplicate any of that here. The newest crates — `crates/pex/` (M47.2 compiled-
Papyrus `.pex` decompiler), `crates/save/` (M45 full-ECS snapshot save/load), and
the expanded `crates/scripting/` (M47.1/M47.2 recognizer chain) — are young code
that has not yet seen a debt sweep; the dimensions below should reach them.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 9.
- `--depth shallow|deep`: `shallow` = surface counts + worst offenders; `deep` = per-instance triage with a concrete fix proposal. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: one of the 9 below.
- **Age** (when relevant): commit hash + date the debt landed (`git log -L` / `git blame`).
- **Effort**: trivial (≤30 min) | small (≤2 h) | medium (≤1 day) | large (>1 day, decompose first).
- **ID convention**: `TD<dim>-NNN` (e.g. `TD7-050` = Dim 7 Doc Rot, finding 50). The
  path-validation gate (`_audit-validate.sh`, #1114) was itself a `TD7-*` finding —
  recurring stale-path findings are what motivated the gate.

## Severity for Tech Debt

Tech-debt findings default to **LOW** (see `_audit-severity.md`). Promote only on amplification:

| Promotion Trigger | Floor |
|-------------------|-------|
| Duplicated logic with divergent bug-fix history (one branch fixed, the other regressed) | MEDIUM |
| `unimplemented!()` / `todo!()` / `panic!("not …")` reachable from a shipped CLI flag or smoke test | MEDIUM |
| `#[ignore]`d test that guards a fix from a closed CRITICAL/HIGH issue | MEDIUM |
| Stale doc/audit baseline that misled an audit in the last 90 days | MEDIUM |
| Magic number that would silently over/underflow under documented use | HIGH |
| Stale `GpuCamera`/`GpuInstance`/`GpuMaterial` size in a doc comment (lockstep-drift bait) | MEDIUM |

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`.
2. `mkdir -p /tmp/audit/tech-debt`.
3. Dedup baseline:
   ```bash
   gh issue list --repo matiaszanolli/ByroRedux --limit 500 --state all --label tech-debt --json number,title,state > /tmp/audit/tech-debt/issues_all.json
   ```
4. Scan `docs/audits/` for prior `AUDIT_TECH_DEBT_*.md` (diff direction, not re-litigation).
5. Snapshot totals so the next audit can diff:
   ```bash
   {
     echo "TODO/FIXME/HACK/XXX:   $(grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux | wc -l)"
     echo "allow(dead_code):      $(grep -RInE 'allow\(dead_code\)' crates byroredux | wc -l)"
     echo "unimplemented!/todo!(): $(grep -RInE 'unimplemented!|todo!\(\)' crates byroredux | wc -l)"
     echo "#[ignore] tests:        $(grep -RIn '#\[ignore\]' . | wc -l)"
     echo "files >2000 LOC:        $(find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | wc -l)"
   } > /tmp/audit/tech-debt/baseline.txt
   ```
   Orientation only (will drift — re-run, never quote): the marker total runs ~20,
   `unimplemented!/todo!()` is currently **0** (the engine prefers explicit
   fallbacks over panics — a fresh `todo!()` is therefore notable), `#[ignore]`
   runs in the low-hundreds (mostly Vulkan/smoke gating, not debt), and the
   >2000-LOC set is ~6 files (Dim 1).

## Phase 2: Dimension Agents

Ordered by debt impact: complexity and duplication compound across every future
edit; doc/audit rot misdirects the *next* audit; markers and dead code are
cheap. Each agent writes `/tmp/audit/tech-debt/dim_<N>.md`.

### Dimension 1: File / Function / Module Complexity
The highest-leverage debt: an oversized file taxes every edit, review, and merge.

**Discovery**:
```bash
find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | sort -rn
```
Session 34/35/36 (2026-05) split the original oversized set (acceleration.rs,
dispatch_tests.rs, cell/tests.rs, draw.rs, scene_buffer.rs, context/mod.rs,
import/mesh.rs, blocks/collision.rs, nif/anim.rs) into submodules — **all of
those are closed; do not re-file them.** Membership has since turned over: the
two big Vulkan-context files *grew* after re-split, and several `byroredux/`
files crossed 2000. Re-run the command; the threshold is **2000 LOC** (the
Session-34 split target). Whatever it lists today is the live set — including any
file the skill once cited as a *success* (a previously-split module can grow back
over threshold).

**Per oversized file, propose a split AXIS by responsibility** (not by line count):
- A Vulkan `context/` file → per-pass recording groups (geometry / RT / denoise /
  composite / overlay) or struct+new() vs Drop vs accessors. Vulkan-recording
  splits are render-pass-adjacent — see `feedback_speculative_vulkan_fixes.md`
  before proposing barrier/order changes.
- `byroredux/src/asset_provider.rs` → BSA/BA2 resolution vs TextureProvider vs mesh extraction.
- `byroredux/src/main.rs` → App/ApplicationHandler event loop vs system registration vs boot/config.
- `byroredux/src/commands/` → console-command groups, already split per-domain (world_info / assets / view / scene / shared) under #1323; check the submodules stay cohesive, not re-bloated.
- `crates/nif/src/blocks/particle.rs` → typed emitter/ctlr structs vs the opaque `NiPSysBlock` fallback vs grow/fade modifiers.

**Also flag**: functions >200 LOC (propose extraction); match arms >50 cases
(want a lookup table); nesting depth >5 (state-machine extraction); a `mod.rs` /
`lib.rs` with >20 `pub use` (doing two jobs). `cargo +nightly clippy --all-targets
-- -W clippy::cognitive_complexity` if available, else inspect the worst offenders.

### Dimension 2: Logic Duplication
CLAUDE.md global policy is explicit: *improve existing code, never duplicate logic.*
Every finding must name a concrete consolidation site.

**Discovery**: target subsystems with N>1 sibling files, then read for repeated scaffolding:
```bash
ls crates/nif/src/blocks/*.rs crates/plugin/src/esm/records/**/*.rs crates/renderer/src/vulkan/*.rs byroredux/src/cell_loader/*.rs
```
**Look for**:
- Block-parser scaffolding repeated across `crates/nif/src/blocks/` (header read → field read → fixup) that should funnel through a shared helper/macro.
- Texture-upload chains (BC1/BC3/BC5/RGBA) duplicated in `crates/renderer/src/vulkan/`.
- The same image-layout barrier sequence repeated per render pass.
- `vk::WriteDescriptorSet` builder boilerplate.
- ESM sub-record parse loops repeated across `crates/plugin/src/esm/records/`.
- Z-up → Y-up coordinate flips reimplemented outside the canonical homes
  (`crates/nif/src/import/coord.rs`, `crates/nif/src/anim/coord.rs`) — any other
  call site is a leak.

### Dimension 3: Stale Documentation & Comments
Doc rot is high-impact debt because it misleads the *next* reader and the *next*
audit. **Run the path gate first** (it is also Dim 9's input):
```bash
.claude/commands/_audit-validate.sh
```
Any STALE refs it prints are auto-eligible findings (effort: trivial). Then
sweep for content rot the gate cannot see:
- **Numeric claims in doc comments that drift from a pinned test.** The canonical
  trap: `GpuCamera` / `GpuInstance` / `GpuMaterial` byte sizes and `Vertex::SIZE`.
  Do NOT trust prose — cross-check against the layout test, whose value is
  authoritative and whose *name* may itself be stale:
  ```bash
  grep -rn "fn gpu_camera_is\|fn gpu_instance_is\|assert_eq.*size_of" crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs
  ```
- Doc comments naming renamed/deleted symbols. The recurring one: the **deleted
  render-time `Material::classify_pbr`** (PBR resolution moved to the parse-time
  NIFAL boundary). Several doc-comments in `crates/core/src/ecs/components/material.rs`
  still name it — each must frame it as *deleted/historical*, never as a live
  entry point. Enumerate and read each:
  ```bash
  grep -n "classify_pbr" crates/core/src/ecs/components/material.rs
  ```
  The surviving symbols are the free function `classify_pbr_keyword` and the
  method `Material::resolve_pbr`; `metalness`/`roughness` are plain resolved `f32`.
  (This overlaps Dim 8 — report material doc rot under Dim 3.)
- ROADMAP.md milestones marked "in progress" whose issues are all closed (or vice versa) — cross-check `git log` / `gh issue`.
- **`docs/feature-matrix.md` lags shipped milestones** (known doc-rot target). Its
  "Save / load (M45)" row still reads "unstarted" and "Full Papyrus transpiler
  (M47.2)" reads "transpiler unstarted", but M45 + M45.1 (`crates/save/`) and the
  M47.2 compiled-`.pex` recognizer slice (`crates/pex/`, `crates/scripting/`) all
  shipped (`git log --grep M45`, `--grep M47.2`). Flag each lagging row; the matrix
  is a status floor, not a record of what exists.
- HISTORY.md entries referencing later-reverted work.
- README.md command examples whose flags/paths changed.
- `docs/legacy/` references to Gamebryo source paths that moved.
- `crates/renderer/shaders/triangle.frag` doc comments quoting outdated GPU struct byte sizes — cross-check the layout test, not the prose.

**Path convention (post-#1114)**: a backticked `.ext` path in any audit-*.md or
this file asserts "exists right now". Forward-looking (not-yet-created) or
backwards-looking (deleted) refs must NOT use backticks. The gate fails on any
backticked path that does not resolve. The gate now globs both the shared
`.claude/commands/_audit-*.md` files AND every `.claude/commands/audit-*/SKILL.md`
subdir (so paths in *this very file* ARE gate-covered) — run it before committing.

### Dimension 4: Audit-Finding Rot
The audit infrastructure decays like any other code, and stale baselines actively
misdirect future audits.
**Discovery**:
```bash
.claude/commands/_audit-validate.sh            # structural path gate (#1114)
ls .claude/commands/audit-*.md docs/audits/
```
- STALE refs from the gate that live in *other* audit skills → Dim 4 findings (trivial).
- Symbol-anchor refs the gate cannot verify (e.g. `crates/audio/src/lib.rs::drain_pending_oneshots`) — spot-check the symbol still exists.
- "Existing: #NNN" callouts in skills where the issue is now CLOSED — reframe as a closed-state baseline.
- Skill files quoting a dimension count ("all N dimensions") that no longer matches the live list.
- `docs/audits/` reports >90 days old whose CRITICAL/HIGH findings are not all triaged on GitHub.
- **Do NOT flag** `.claude/issues/<N>/ISSUE.md` "Status: Open" drift — dropped per
  TD10-001 / #1156: local issue files are immutable snapshots; GitHub is
  authoritative. Query `gh issue view <N> --json state` for live state.

### Dimension 5: Stale Markers (TODO / FIXME / HACK / XXX)
**Discovery**:
```bash
grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux
grep -RInE '(TODO|HACK)' crates/renderer/shaders/
```
**Triage each** (skip markers <30 days old unless they name a closed issue):
- `git blame` for age — anything >6 months gets reported.
- Does it name an issue number? Is that issue still open? Closed issue + live marker → "marker outlived its driver" (delete or reopen).
- Does it name a milestone (M21, M29, …) now complete per ROADMAP.md?
- `// TODO: implement` on a path now reachable from a shipped CLI flag → promote (see severity table).
- **False positives to exclude**: `XXXX` is the ESM extended-size sub-record tag
  (`crates/plugin/src/esm/reader.rs`, `records/misc/magic.rs`) — protocol, not a
  marker. `// FIXME note` referencing a *reference implementation's* FIXME (e.g.
  `crates/bgsm/src/bgem.rs`) is documentation of upstream, not our debt.
- **Must-not-delete**: the third-party attribution block atop
  `crates/renderer/shaders/triangle.frag` (GLSL-PathTracer MIT notice + Burley
  2012 citation, ~first 30 lines). Flag any edit that strips/truncates it — MIT
  requires the notice travel with the code.

### Dimension 6: Stub & Placeholder Implementations
**Discovery**:
```bash
grep -RInE 'unimplemented!|todo!\(\)|panic!\("not ' crates byroredux
grep -RInE '// *(stub|TODO: real|placeholder|not yet)' crates byroredux
```
The first command currently returns **nothing** — the codebase prefers explicit
fallbacks to panics, so any hit is genuinely notable. For each:
- Reachable from a shipped CLI flag or smoke test? → promote to MEDIUM.
- Functions returning `None` / `Vec::new()` / `Default::default()` with a "// stub"/"// TODO: real impl" comment.
- Trait impls with empty bodies that the trait docs say should do work.
- Per-game ESM record coverage in `crates/plugin/src/esm/records/` — fully wired
  vs stubbed per game; cross-check ROADMAP.md per-game compat matrix. (The legacy
  per-game stubs in `crates/plugin/src/legacy/` were removed under #390 — coverage
  now lives in the unified records tree; do not re-file the removed stubs.)
- Console commands in `byroredux/src/commands/` that exist but no-op / print "TODO".

### Dimension 7: Magic Numbers & Hardcoded Constants
**Discovery**: read the version-gate and budget sites; do not regex blindly (most
literals are legitimate).
- Bare numeric literals in `crates/nif/src/blocks/` compared against version codes → should be a `NifVersion` constant.
- Vulkan `MAX_*`/`MIN_*` hardcoded inline → reference `vk::PhysicalDeviceLimits` or a named constant.
- **Shader `#define` provenance**: every shader define is generated from one Rust
  source — `crates/renderer/src/shader_constants_data.rs` is `include!`d by both
  `crates/renderer/src/shader_constants.rs` and `crates/renderer/build.rs` (which
  emits `shaders/include/shader_constants.glsl`). The generated-header infra
  exists; the check is **"every shader `#define` is sourced from
  `shader_constants_data.rs`; flag any literal that bypasses it"** (lockstep risk
  HIGH — `feedback_shader_struct_sync.md`).
- **GPU `#[repr(C)]` size literals**: `GpuCamera`, `GpuInstance`, `GpuMaterial`
  sizes are pinned by `gpu_instance_layout_tests.rs`. Flag any inline size literal
  that should reference those tests, and any doc comment quoting an outdated size
  (overlaps Dim 3). Get the live values from the test, not from memory:
  ```bash
  grep -rn "fn gpu_camera_is\|fn gpu_instance_is\|size_of::<Gpu" crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs
  ```
- Frame/ray/cache budgets (`GLASS_RAY_BUDGET`, `MAX_TOTAL_BONES`, `MAX_MATERIALS`, …) scattered vs in one tunable module.
- ESM sub-record sizes hardcoded (`if data.len() == 24`) → named constant from the record struct.
- **Do NOT flag** protocol-defined magic: FourCC tags, BSA/NIF/BA2 magic, Vulkan format enums.

### Dimension 8: Dead Code & Backwards-Compat Cruft
**Discovery**:
```bash
grep -RInE 'allow\(dead_code\)' crates byroredux
grep -RInE '#\[deprecated\]|// *removed:|_unused|fn .*_unused' crates byroredux
cargo machete 2>/dev/null || echo "cargo machete not installed — scan Cargo.toml deps vs use stmts"
```
- Each `#[allow(dead_code)]` — actually called now, or still dead? Delete if dead.
- `pub fn` in a private module no one imports (`cargo +nightly rustc -- -W unused`).
- `mod.rs`/`lib.rs` re-exports with no downstream consumer.
- `_`-prefixed params that survived a refactor (CLAUDE.md: delete, don't rename to `_var`).
- `// removed: …` breadcrumbs (CLAUDE.md: delete completely, no breadcrumbs).
- Re-exports of deleted types kept "for compatibility" — ByroRedux has no external consumers yet, so these are pure rot.
- `Cargo.toml` feature flags with only one branch (always-on/always-off) → remove the flag.
- `#[deprecated]` items with no consumers → delete, don't deprecate.
- **Do NOT flag**: `cfg(test)`/`cfg(debug_assertions)`-gated code, FFI boundary
  functions, or public API of a workspace-internal crate a future binary will
  consume (note such cases rather than deleting).

### Dimension 9: Test Hygiene
**Discovery**:
```bash
grep -RIn '#\[ignore\]' . | grep -v target/
```
Most `#[ignore]`s gate Vulkan/smoke tests that need a GPU or on-disk game data —
those are **not** debt. Triage the rest:
- Each `#[ignore]` test: referenced issue still open? If it guards a closed CRITICAL/HIGH fix → MEDIUM (severity table).
- Tests with only smoke assertions (`assert!(result.is_ok())` and nothing else) — should assert on values.
- Commented-out assertions inside otherwise-passing tests (`// assert_eq!(…)`).
- `#[cfg(feature = "…")]`-gated tests where the feature is never enabled in CI.
- Tests that `println!` without a follow-up assert.
- `byroredux/tests/golden_frames.rs` (opts into `--ignored`) — still runnable, golden image current.
- Cross-reference "must not regress" lines in other audit skills (e.g. `audit-performance`) — each named regression test still present and not `#[ignore]`d.

## Cross-Dimension Dedup

A TODO inside a dead function reports under Dim 8 (Dead Code), not also Dim 5.
Material doc rot reports under Dim 3, not also Dim 8. A stale GPU-size doc comment
reports under Dim 3; a stale GPU-size *code literal* under Dim 7. NIFAL/material
*translation correctness* is out of scope here — that is `/audit-nifal`. This
audit only owns the *debt* around that tier (dead code, stale doc, leftover
breadcrumbs).

## Phase 3: Merge

1. Read all `/tmp/audit/tech-debt/dim_*.md`.
2. Combine into `docs/audits/AUDIT_TECH_DEBT_<TODAY>.md`:
   - **Executive Summary** — findings by severity + delta vs `baseline.txt`.
   - **Baseline Snapshot** — the Phase-1 counts, so the next audit can diff.
   - **Top 10 Quick Wins** — trivial/small effort, immediate readability or compile-time payoff.
   - **Top 5 Medium Investments** — file/function splits, duplication consolidations.
   - **Findings** — by severity (HIGH → MEDIUM → LOW), then by dimension.
   - **Deferred** — findings gated on an in-progress milestone; name the gating milestone.
3. Remove cross-dimension duplicates per the rules above.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/tech-debt`.
2. Tell the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_TECH_DEBT_<TODAY>.md`.

## GitHub Label

Findings publish under the `tech-debt` label (plus the standard `<severity>` and
`<domain>` labels). It is registered in the repo — `/audit-publish` applies it
automatically when a finding's audit type is `TECH_DEBT`.
