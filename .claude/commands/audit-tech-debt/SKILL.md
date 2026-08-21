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

See `.claude/commands/_audit-common.md` for project layout, the crate roster,
methodology, deduplication, context rules, severity, and finding format. Do not
duplicate any of that here. The newest crates — `crates/pex/` (M47.2 compiled-
Papyrus `.pex` decompiler), `crates/save/` (M45 full-ECS snapshot save/load),
`crates/hkx/` (M47.2 Havok packfile reader for the MQ101 cinematic slice), and
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
5. **Production-LOC helper** (#3081 / TD4-2026-08-16-01 — Dim 1's actual
   subject is production complexity, not file length; a file that is long
   because of bulk inline tests is not the debt this dimension hunts).
   Define once, reuse for both the snapshot below and Dim 1's own discovery:
   ```bash
   # Production LOC estimate for one .rs file.
   #
   # Pure-test files by this codebase's own naming convention (`tests.rs` /
   # `*_tests.rs`, or anything under a `tests/` dir — e.g.
   # `acceleration/tests/predicates_tests.rs`,
   # `scene_buffer/gpu_instance_layout_tests.rs`)
   # report 0: their `#[cfg(test)] #[path = "..."] mod <name>;` gate lives in
   # the PARENT file that declares them, so no in-file marker exists to
   # detect it from the file's own content.
   #
   # Everything else: total LOC minus every line inside a #[cfg(test)]-gated
   # BRACE-DELIMITED item, tracked by brace depth so multiple scattered test
   # blocks in one file are all excluded (a first-#[cfg(test)]-occurrence
   # cutoff badly undercounts a file like draw.rs, whose first #[cfg(test)]
   # sits ~200 lines into a 4700-line file, well before the bulk of its
   # production code). A #[cfg(test)] attribute on a `;`-terminated item
   # (an external `mod tests;` declaration, a `#[path]` attribute line, or a
   # test-only `use`) has no block to track — only that one line is excluded.
   prod_loc() {
       case "$1" in
           */tests/*|*tests.rs) echo 0; return ;;
       esac
       awk '
           pending && /;/ && !/\{/ { pending = 0; next }
           /^#\[cfg\(test\)\]/ { pending = 1; next }
           pending && /\{/ {
               in_test = 1; depth = 0
               n = gsub(/\{/, "{"); depth += n
               n = gsub(/\}/, "}"); depth -= n
               pending = 0
               if (depth == 0) in_test = 0
               next
           }
           in_test {
               n = gsub(/\{/, "{"); depth += n
               n = gsub(/\}/, "}"); depth -= n
               if (depth <= 0) in_test = 0
               next
           }
           { prod++ }
           END { print prod + 0 }
       ' "$1"
   }
   ```
6. Snapshot totals so the next audit can diff:
   ```bash
   {
     echo "TODO/FIXME/HACK/XXX:   $(grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux | wc -l)"
     echo "allow(dead_code):      $(grep -RInE 'allow\(dead_code\)' crates byroredux | wc -l)"
     echo "unimplemented!/todo!(): $(grep -RInE 'unimplemented!|todo!\(\)' crates byroredux | wc -l)"
     echo "#[ignore] tests:        $(grep -RIn '#\[ignore\]' . | wc -l)"
     echo "files >2000 production LOC: $(for f in $(find crates byroredux -name '*.rs'); do echo "$(prod_loc "$f")"; done | awk '$1>2000' | wc -l)"
     echo "test files >2000 total LOC (lower priority, separate bucket): $(find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | wc -l | xargs -I{} echo {})"
   } > /tmp/audit/tech-debt/baseline.txt
   ```
   Orientation only (will drift — re-run, never quote): the marker total runs ~20,
   `unimplemented!/todo!()` is currently **0** (the engine prefers explicit
   fallbacks over panics — a fresh `todo!()` is therefore notable), `#[ignore]`
   runs in the mid-hundreds (mostly Vulkan/smoke gating, not debt).
   The **production**->2000-LOC set (Dim 1's actual subject, re-verified
   2026-08-19 with *prod_loc*) is currently 4 files: `context/mod.rs`
   (~4060), `context/draw.rs` (~3490 — most of its file-total length, not
   merely its production share: only a few small scattered `#[cfg(test)]`
   blocks sit inside an otherwise huge production body), `volumetrics.rs`
   (~2770), and `texture_registry.rs` (~2010). That last one is a real
   disagreement with this issue's own filed evidence table (which reported
   `texture_registry.rs` production at 838 — majority-test): re-checking the
   file directly finds only 3 `#[cfg(test)]` markers total, all within the
   last ~100 lines, two of which are `#[path = "..."] mod tests;`
   declarations pointing at separate files — the file's own content is
   genuinely ~2010 lines of production texture-registry logic (samplers,
   path normalisation, bindless acquire/release). Filed as a real Dim 1
   finding here rather than silently adopting a figure this check disproves.
   `material.rs` (#2257, ~1330 production) and `gpu_instance_layout_tests.rs`
   (0 production — reached only via an external `#[cfg(test)] mod`
   declaration in its parent) both confirm as false positives under the OLD
   total-LOC recipe. The separate total-LOC->2000 bucket (test-heavy files,
   lower priority — report but do not auto-file as Dim 1) currently also
   includes `svgf.rs`, `misc/world.rs`,
   `crates/physics/src/world.rs`, `env_translate.rs`, `cornell.rs`, `mesh.rs`,
   `import/collision/shape.rs`, and `plugin/tests/parse_real_esm.rs` — check
   each with *prod_loc* before filing; only a production count over 2000
   belongs in Dim 1. Note #2258/#2259 (2026-08-03, `record_post_passes` /
   `build_tlas` decomposition) extracted helpers *within* `post_passes.rs` /
   `tlas.rs`, which stayed well under 2000 LOC before and after — file-level
   crossings and function-level splits are independent signals; don't assume
   one moves the other.

## Phase 2: Dimension Agents

Ordered by debt impact: complexity and duplication compound across every future
edit; doc/audit rot misdirects the *next* audit; markers and dead code are
cheap. Each agent writes `/tmp/audit/tech-debt/dim_<N>.md`.

### Dimension 1: File / Function / Module Complexity
The highest-leverage debt: an oversized file taxes every edit, review, and merge.

**Discovery**: two buckets, not one (#3081 / TD4-2026-08-16-01 — total LOC is a
proxy for the property this dimension actually hunts, production complexity,
and the two had decoupled: 7 of 11 files the old single-bucket recipe reported
were majority-test, 2 were pure-test files with zero production code). Use the
*prod_loc* helper defined in Phase 1, step 5:
```bash
# Primary bucket — the dimension's actual subject. File real findings from this.
for f in $(find crates byroredux -name '*.rs'); do
    p=$(prod_loc "$f")
    [ "$p" -gt 2000 ] && echo -e "$p\t$f"
done | sort -rn

# Secondary bucket — test-heavy files, lower priority. Report but do not
# auto-file: only escalate one of these into a Dim 1 finding if its OWN
# prod_loc figure (above) also crosses 2000.
find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | sort -rn
```
Session 34/35/36 (2026-05) split the original oversized set (acceleration.rs,
dispatch_tests.rs, cell/tests.rs, draw.rs, scene_buffer.rs, context/mod.rs,
import/mesh.rs, blocks/collision.rs, nif/anim.rs) into submodules — **all of
those are closed; do not re-file them.** Membership has since turned over: the
two big Vulkan-context files *grew* after re-split, and several `byroredux/`
files crossed 2000 (mostly on the test-only bucket now — see Phase 1 step 6's
orientation note for the live production-bucket membership). Re-run both
commands; the threshold is **2000 LOC** (the Session-34 split target) measured
against **production** LOC for the primary bucket. Whatever it lists today is
the live set — including any file the skill once cited as a *success* (a
previously-split module can grow back over threshold).

**Per oversized file, propose a split AXIS by responsibility** (not by line count):
- A Vulkan `context/` file → per-pass recording groups (geometry / RT / denoise /
  composite / overlay) or struct+new() vs Drop vs accessors. Vulkan-recording
  splits are render-pass-adjacent — see `feedback_speculative_vulkan_fixes.md`
  before proposing barrier/order changes.
- `byroredux/src/asset_provider/` → BSA/BA2 resolution vs TextureProvider vs mesh extraction.
- ~~`byroredux/src/main.rs` → App/ApplicationHandler event loop vs system registration vs boot/config.~~ **DONE (#2731)** — main.rs is 834 LOC; the ApplicationHandler moved to `byroredux/src/app_events.rs` and the frame driver to `byroredux/src/app_frame.rs`. Do not re-propose this split. The live oversized-file candidates in the binary are now `byroredux/src/interaction.rs` (1356 LOC, and it mixes UI input routing with the canonical player-action/activation producer — a real seam) and `byroredux/src/app_events.rs` (1039 LOC).
- `byroredux/src/commands/` → console-command groups, already split per-domain (world_info / assets / view / scene / shared) under #1323; check the submodules stay cohesive, not re-bloated.
- `crates/nif/src/blocks/particle.rs` → typed emitter/ctlr structs vs the opaque `NiPSysBlock` fallback vs grow/fade modifiers.
- `crates/nif/src/import/collision/mod.rs` → split per bhk shape family (primitive/compound/mesh/compressed), mirroring `crates/nif/src/blocks/collision/`.
- `crates/core/src/ecs/resources/mod.rs` → partially split already (`SkinSlotPool` extracted to `skin_slot_pool.rs` under #1869; `mod.rs` now 1210 LOC, under threshold). Split further per resource domain (rendering/world/audio/scripting) only if it re-bloats.
- Actor record split per NPC_ data-group (13 groups) — done (#2055): `crates/plugin/src/esm/records/actor/mod.rs` (+ `tests.rs`).

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
- **`docs/feature-matrix.md` — re-check each milestone row against shipped code**
  (`git log --grep M45`, `--grep M47.2`, …). The known M45/M47.2 lag was fixed on
  2026-06-21 (the Save/load row was removed, the M47.2 row now reads "✓ `.pex`
  recognizer slice … full transpiler deferred"), so those are clean — but the
  matrix is a *status floor, not a record of what exists*, so any future
  milestone can drift the same way. Flag any row whose status contradicts the
  crate that implements it.
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
ls .claude/commands/_audit-*.md .claude/commands/audit-*/SKILL.md docs/audits/
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

## GitHub Labels

Findings publish under the `tech-debt` label (plus the standard `<severity>` and
`<domain>` labels). It is registered in the repo — `/audit-publish` applies it
automatically when a finding's audit type is `TECH_DEBT`.

Two sibling kind labels split the bucket further (both registered 2026-08-21) —
apply the one that matches the defect instead of leaving everything under bare
`tech-debt`:

- **`doc-rot`** — documentation drifted from code: a stale ROADMAP row, a SKILL
  doc naming a deleted symbol, a comment describing removed behaviour. These
  publish as `documentation` (type), not `bug`.
- **`test-gap`** — missing, vacuous, or non-asserting coverage: an `#[ignore]`d
  test with no data gate, a test whose assertions are satisfied by a sibling, an
  entry point with zero tests.

Pure debt — dead code, duplication, magic numbers, oversized files, stale
markers — stays `tech-debt` + `bug`.
