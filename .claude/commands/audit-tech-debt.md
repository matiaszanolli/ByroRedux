---
description: "Audit accumulated technical debt — stale markers, dead code, duplication, magic numbers, stub impls, doc rot, oversized files"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Tech-Debt Audit

Audit ByroRedux for accumulated technical debt: code that compiles, passes tests, and ships, but quietly raises the cost of every future change. The goal is **not** to find correctness bugs (other audits cover that) — it's to surface decay that has crept in since the last cleanup pass.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,5`). Default: all 10.
- `--depth shallow|deep`: `shallow` = surface counts and worst offenders; `deep` = file-by-file with concrete fix proposals. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Stale Markers | Dead Code | Logic Duplication | Magic Numbers | Stub Implementations | Test Hygiene | Stale Documentation | Backwards-Compat Cruft | File / Function Complexity | Audit-Finding Rot
- **Age** (when applicable): commit hash + date the debt was introduced (use `git log -L` or `git blame`)
- **Effort**: trivial (≤30 min) | small (≤2 h) | medium (≤1 day) | large (>1 day, decompose first)

## Severity Guidance for Tech Debt

Tech-debt findings are almost always **LOW** under the standard severity scale (see `_audit-severity.md`). Promote when there's amplification:

| Promotion Trigger | Floor |
|-------------------|-------|
| Duplicated logic with divergent bug-fix history (one branch fixed, the other regressed) | MEDIUM |
| `unimplemented!()` / `todo!()` reachable from a shipped CLI / smoke test | MEDIUM |
| `#[ignore]`d test guarding a fix from a closed CRITICAL/HIGH issue | MEDIUM |
| Stale doc/audit baseline that has misled an audit in the last 90 days | MEDIUM |
| Magic number that would silently overflow / underflow under documented use cases | HIGH |

Default tech-debt findings to LOW unless one of the above fires.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`
2. `mkdir -p /tmp/audit/tech-debt`
3. Fetch dedup baseline:
   ```bash
   gh issue list --repo matiaszanolli/ByroRedux --limit 200 --label tech-debt --json number,title,state > /tmp/audit/tech-debt/issues_open.json
   gh issue list --repo matiaszanolli/ByroRedux --limit 500 --state all --label tech-debt --json number,title,state > /tmp/audit/tech-debt/issues_all.json
   ```
4. Scan `docs/audits/` for prior tech-debt reports (`AUDIT_TECH_DEBT_*.md`)
5. Snapshot current totals as baseline (so the report can show direction):
   ```bash
   {
     echo "TODO/FIXME/HACK/XXX: $(grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux | wc -l)"
     echo "allow(dead_code): $(grep -RInE 'allow\(dead_code\)' crates byroredux | wc -l)"
     echo "unimplemented!/todo!(): $(grep -RInE 'unimplemented!|todo!\(\)' crates byroredux | wc -l)"
     echo "#[ignore] tests: $(grep -RIn '#\[ignore\]' . | wc -l)"
     echo "files >2000 LOC: $(find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1 > 2000 && $2 != "total"' | wc -l)"
   } > /tmp/audit/tech-debt/baseline.txt
   ```

## Phase 2: Launch Dimension Agents

### Dimension 1: Stale Markers (TODO / FIXME / HACK / XXX)
**Entry points**: `crates/`, `byroredux/` (all `.rs`), `crates/renderer/shaders/` (all `.glsl`/`.comp`/`.vert`/`.frag`)
**Checklist**:
- Each marker: how old is it? (`git blame` to commit + date — anything older than 6 months gets reported)
- Does the marker reference an issue number? Is that issue still open?
  - Closed issue + marker still present → "marker outlived its driver" (delete or reopen)
- Does the marker reference a milestone (M21, M29, etc.) that is now complete (per ROADMAP.md)?
- Are there `// TODO: implement` markers in code paths that are now reachable from a shipped CLI flag?
- Skip markers from the last 30 days unless they reference a closed issue

**Output**: `/tmp/audit/tech-debt/dim_1.md`

### Dimension 2: Dead Code & Unused Surface
**Entry points**: `crates/`, `byroredux/`
**Checklist**:
- Every `#[allow(dead_code)]` — is the code actually called now, or still dead?
- Every `pub fn` in a private module that no other module imports (run `cargo +nightly rustc -- -W unused`)
- Re-exports through `mod.rs` / `lib.rs` that no downstream consumer uses
- `_` -prefixed unused parameters that survived a refactor (`fn foo(_world: &World)` where `World` is no longer needed)
- Unused crate dependencies (`cargo machete` if installed; otherwise scan `Cargo.toml` against `use` statements)
- Trait impls for types no other code constructs (e.g., a `Display` impl that's never called)
- **Don't flag**: `cfg(test)` / `cfg(debug_assertions)`-gated code, FFI boundary functions, public API of a workspace-internal crate that future binaries will consume (note in CLAUDE.md or ROADMAP.md)

**Output**: `/tmp/audit/tech-debt/dim_2.md`

### Dimension 3: Logic Duplication
**Entry points**: any subsystem with N>1 similar files (`crates/nif/src/blocks/`, `crates/plugin/src/esm/records/`, `crates/renderer/src/vulkan/`, `byroredux/src/cell_loader/`)
**Checklist**:
- Identical or near-identical block-parser scaffolding across `crates/nif/src/blocks/*.rs` (header read → field read → fixup) — is there a `parse_block` macro/helper this should funnel through?
- Repeated texture-upload paths in `crates/renderer/src/vulkan/` (BC1/BC3/BC5/RGBA chains)
- Same Vulkan barrier sequence repeated per render pass (image layout transitions)
- Repeated descriptor set update boilerplate (vk::WriteDescriptorSet builders)
- ESM record parsers in `crates/plugin/src/esm/records/` — common subrecord parse loops that should share a helper
- Coordinate-system flip (Z-up → Y-up) reimplemented at multiple call sites (`crates/nif/src/import/coord.rs` is the canonical home — anything outside is a leak)
- **Cross-reference user policy** (CLAUDE.md global): "Always prioritize improving existing code rather than duplicating logic." Each duplication finding must propose a specific consolidation site.

**Output**: `/tmp/audit/tech-debt/dim_3.md`

### Dimension 4: Magic Numbers & Hardcoded Constants
**Entry points**: `crates/nif/src/blocks/`, `crates/renderer/src/vulkan/`, shaders
**Checklist**:
- Bare numeric literals in `crates/nif/src/blocks/*.rs` that compare against version codes (should be a `NifVersion` constant)
- Vulkan `MAX_*` / `MIN_*` numbers hardcoded inline (should reference `vk::PhysicalDeviceLimits` queries or named constants)
- Shader `#define` values duplicated as Rust constants — are they kept in lockstep? Is there a shader-binding-table or generated header? (Lockstep risk is HIGH — see `feedback_shader_struct_sync.md`)
- Frame-budget / ray-budget / cache-size numbers (e.g., `GLASS_RAY_BUDGET = 8192`, `MAX_TOTAL_BONES`, `MAX_MATERIALS = 4096`) — are they all in one tunable module, or scattered?
- ESM record sub-record sizes hardcoded (e.g., `if data.len() == 24`) — should map to a named constant from the record struct
- **Don't flag**: protocol-defined magic (4-char FourCC tags, BSA/NIF magic numbers, Vulkan format enums) — these are spec, not arbitrary

**Output**: `/tmp/audit/tech-debt/dim_4.md`

### Dimension 5: Stub & Placeholder Implementations
**Entry points**: `crates/`, `byroredux/`
**Checklist**:
- Every `unimplemented!()` and `todo!()` — is the call site reachable from a shipped CLI flag or smoke test?
- `panic!("not yet")` / `panic!("not implemented")` — same reachability check
- Functions that return `None` / `Vec::new()` / `Default::default()` with a comment like "// TODO: real impl" or "// stub"
- Trait impls with empty method bodies that should do work (check against trait docs for required behavior)
- `crates/plugin/src/legacy/{tes3,tes4,tes5,fo4}.rs` parser stubs — which records are fully wired vs still stubbed? (Cross-check against ROADMAP.md per-game compat matrix)
- Console commands in `byroredux/src/commands.rs` that exist but are no-ops or print "TODO"

**Output**: `/tmp/audit/tech-debt/dim_5.md`

### Dimension 6: Test Hygiene
**Entry points**: `**/*_tests.rs`, `**/tests/**`, `byroredux/tests/`
**Checklist**:
- Every `#[ignore]` test: is there a referenced issue? Is that issue still open?
- Tests with **only** smoke assertions (`assert!(result.is_ok())` and nothing else) — should assert on returned values
- Commented-out assertions inside otherwise-passing tests (`// assert_eq!(...)`)
- `#[cfg(feature = "...")]`-gated tests where the feature is never enabled in CI
- Tests that print rather than assert (`println!("got {x}")` with no follow-up `assert_eq!`)
- Tests with `unwrap()` everywhere — are they testing the failure paths they need to?
- Golden-frame tests (`byroredux/tests/golden_frames.rs`) marked `--ignored` — confirm they're still runnable and the golden image is current
- **Cross-reference baseline notes** in audit skills (e.g., `audit-performance.md`'s "must not regress" lines) — is each mentioned regression test still present and not `#[ignore]`d?

**Output**: `/tmp/audit/tech-debt/dim_6.md`

### Dimension 7: Stale Documentation & Comments
**Entry points**: `docs/`, `ROADMAP.md`, `HISTORY.md`, `README.md`, `CLAUDE.md`, `.claude/commands/audit-*.md`, doc comments in source
**Checklist**:
- File path / line refs in audit skills that drifted after the Session 34 split (`feedback_session34_layout.md` is the translation map — apply it)
- Doc comments that reference renamed types (e.g., `// See OldStruct` where `OldStruct` was renamed)
- "76-byte Vertex" -style numerical claims in doc comments that no longer match `Vertex::SIZE` (commit 1c388e5 fixed one batch of these — sweep for the rest)
- ROADMAP.md milestones marked "in progress" but the underlying issues are all closed (or vice versa)
- HISTORY.md entries that reference issues that were later reverted
- README.md command examples that no longer work (`cargo run` flag changed, BSA path syntax changed)
- `docs/legacy/` references to Gamebryo source paths that may have moved

**Output**: `/tmp/audit/tech-debt/dim_7.md`

### Dimension 8: Backwards-Compat Cruft
**Entry points**: `crates/`, `byroredux/`, `Cargo.toml` files
**Checklist**:
- `_unused`-renamed parameters or fields that survived a refactor (CLAUDE.md is explicit: don't rename to `_var`, delete it)
- `// removed: ...` comment markers (CLAUDE.md again: delete completely, no breadcrumbs)
- Re-exports of deleted types kept "for compatibility" — but ByroRedux has no external consumers yet, so these are just rot
- Feature flags in `Cargo.toml` for features that have only one branch (always-on or always-off) — remove the flag
- Deprecated `#[deprecated]` items with no consumers — delete instead of deprecate
- Cell-loader `nif_import_registry` legacy paths — given the post-Session-34 split, are any old call sites still on the deprecated route?

**Output**: `/tmp/audit/tech-debt/dim_8.md`

### Dimension 9: File / Function / Module Complexity
**Entry points**: `crates/`, `byroredux/`
**Checklist**:
- List `.rs` files >2000 LOC (Session 34 split target was 2000):
  ```bash
  find crates byroredux -name '*.rs' -exec wc -l {} + | awk '$1 > 2000 && $2 != "total"' | sort -rn
  ```
  As of 2026-05-13: `acceleration.rs` 4200, `dispatch_tests.rs` 3667, `cell/tests.rs` 3329, `draw.rs` 2554, `scene_buffer.rs` 2367, `context/mod.rs` 2348, `import/mesh.rs` 2212, `blocks/collision.rs` 2162, `nif/anim.rs` 2101. As of 2026-05-14 (post-Session-36 split sweep — #29e9f45/bd45caa/9c1f723/1fe5321/fe47706/014adc8/ca81c19): 7 of 9 closed; remaining are `context/draw.rs` 2571 and `context/mod.rs` 2363 (both Vulkan-recording-adjacent, see `feedback_speculative_vulkan_fixes.md`). For each, propose a split axis (by submodule responsibility, not by line count alone).
- Functions >200 LOC — propose extraction
- Match arms >50 cases (these usually want a lookup table)
- Nesting depth >5 (often a state-machine extraction candidate)
- Modules with >20 `pub use` re-exports (the module is doing two jobs)
- `cargo +nightly clippy --all-targets -- -W clippy::cognitive_complexity` if available; otherwise visual inspection of the worst-offender list

**Output**: `/tmp/audit/tech-debt/dim_9.md`

### Dimension 10: Audit-Finding Rot
**Entry points**: `.claude/commands/audit-*.md`, `docs/audits/`, `.claude/issues/`
**Checklist**:
- Audit skill "must not regress" baselines (e.g., `audit-performance.md` lines referencing `streaming.rs:286`, `MAX_TOTAL_BONES` location, `scene_buffer/upload.rs::upload_materials`) — verify each line/symbol still exists; if drifted, propose a fix
- "Existing: #NNN" callouts in skill files where the issue is now CLOSED — should the skill prose reference the closed-state baseline differently?
- `.claude/issues/<N>/ISSUE.md` entries where the upstream GitHub issue was closed but the local file still says "Status: Open"
- Audit reports in `docs/audits/` from >90 days ago whose CRITICAL/HIGH findings are not all triaged (open or closed) on GitHub
- Skill files that reference dimension counts (e.g., "all 9 dimensions") that don't match the current dimension list

**Output**: `/tmp/audit/tech-debt/dim_10.md`

## Phase 3: Merge

1. Read all `/tmp/audit/tech-debt/dim_*.md` files
2. Combine into `docs/audits/AUDIT_TECH_DEBT_<TODAY>.md` with structure:
   - **Executive Summary** — Total findings by severity + delta vs `baseline.txt`
   - **Baseline Snapshot** — counts captured in Phase 1, so the next audit can diff
   - **Top 10 Quick Wins** — trivial / small effort, immediate readability or compile-time payoff
   - **Top 5 Medium Investments** — file/function splits, duplication consolidations
   - **Findings** — Grouped by severity (HIGH first, then MEDIUM, then LOW), then by dimension
   - **Deferred** — Findings that depend on milestones still in progress; note the gating milestone
3. Remove cross-dimension duplicates (e.g., a TODO inside a dead function reports under Dim 2, not also Dim 1)

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/tech-debt`
2. Inform user the report is ready
3. Suggest: `/audit-publish docs/audits/AUDIT_TECH_DEBT_<TODAY>.md`

## GitHub Label

This audit publishes findings under the `tech-debt` label (in addition to the standard `<severity>` and `<domain>` labels). The label is registered in the repository — `/audit-publish` will apply it automatically when a finding's audit type is `TECH_DEBT`.
