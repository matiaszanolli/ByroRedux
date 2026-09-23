---
description: "Audit accumulated technical debt — stale markers, dead code, duplication, magic numbers, stub impls, doc rot, oversized files"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Tech-Debt Audit

Audit ByroRedux for accumulated technical debt: code that compiles, passes tests and ships, but raises the
cost of every future change. Correctness bugs belong to other audits; this one hunts decay since the last
cleanup pass.

**Every dimension below is a DISCOVERY RECIPE, not a finding list.** Instances churn between audits, so each
dimension hands you a command that enumerates *current* instances plus a triage rule. There is no hardcoded
instance list here on purpose — re-run the recipe and report what it surfaces today. Where a `cargo test` /
CI gate already enforces an invariant, the dimension names the guard and aims at what it cannot see.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for layout, crate roster, methodology, dedup, severity and finding
format. Young code has had the fewest sweeps — find it with
`git log --diff-filter=A --since=<last-report-date> --name-only --format= -- 'crates/*/Cargo.toml' 'tools/*/Cargo.toml'`
plus the crates the last report never names (today `crates/sdk`, `crates/mod-runtime`, `crates/menuxml`,
`crates/scripting`, `crates/save`, `crates/hkx`, `crates/spt`, and `tools/`). File real findings there; do not
just note the crate is young. `crates/cxx-bridge` and `crates/platform` are deliberate placeholders owned here:
check they have not grown a second job or silent consumers, not that they are small.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers (e.g. `1,3,5`). Default: all 9.
- `--depth shallow|deep`: `shallow` = surface counts + worst offenders; `deep` = per-instance triage with a concrete fix proposal. Default `deep`.

## Extra Per-Finding Fields

- **Dimension**: one of the 9 below.
- **Age** (when relevant): commit hash + date the debt landed (`git log -L` / `git blame`).
- **Effort**: trivial (≤30 min) | small (≤2 h) | medium (≤1 day) | large (>1 day, decompose first).
- **ID convention**: `TD<dim>-<date>-NN` (e.g. `TD3-2026-09-05-02` = Dim 3 Stale Documentation, finding 2 of that report).

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

1. Parse `$ARGUMENTS`. 2. `mkdir -p /tmp/audit/tech-debt`.
3. Dedup baseline:
   `gh issue list --repo matiaszanolli/ByroRedux --limit 500 --state all --label tech-debt --json number,title,state > /tmp/audit/tech-debt/issues_all.json`
4. Scan `docs/audits/` for prior `AUDIT_TECH_DEBT_*.md` (diff direction, not re-litigation).
5. **Production-LOC helper** (Dim 1's subject is production complexity, not file length — a file long from bulk
   inline tests is not the debt this dimension hunts). Source the checked-in script and run its self-test first;
   a failure means no figure below it can be trusted:
   ```bash
   source .claude/commands/audit-tech-debt/prod_loc.sh
   prod_loc_self_test        # must print "prod_loc self-test: ok"
   ```
   *prod_loc* `<file>` reports 0 for pure-test files (`tests.rs`, `*_tests.rs`, anything under `tests/` — their
   `#[cfg(test)] mod` gate lives in the parent) and otherwise total LOC minus every `#[cfg(test)]`-gated braced
   item, counting braces on code only (comments, strings, raw strings and char literals stripped, #4336).
6. Snapshot totals so the next audit can diff. `tools/` is in scope (four workspace binaries); `tools/nifskope` is
   vendored and pruned:
   ```bash
   {
     echo "markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): $(grep -RInE '(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "allow(dead_code):      $(grep -RInE 'allow\(dead_code\)' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "unimplemented!/todo!(): $(grep -RInE 'unimplemented!|todo!\(\)' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "#[ignore] tests:        $(grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools --exclude-dir=nifskope | wc -l)"
     echo "files >2000 production LOC: $(for f in $(find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*'); do echo "$(prod_loc "$f")"; done | awk '$1>2000' | wc -l)"
     echo "test files >2000 total LOC (lower priority, separate bucket): $(find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | wc -l)"
   } > /tmp/audit/tech-debt/baseline.txt
   ```
   Measured 2026-09-19 (diff direction only, re-run, never quote): markers 22 (16 are `XXXX` false positives),
   `allow(dead_code)` 28, `unimplemented!/todo!()` **0** (a fresh hit is notable), `#[ignore]` 217 (tools-inclusive;
   earlier reports scoped to `crates`+`byroredux` read lower), production >2000 LOC: 3, test-heavy >2000: 52. A
   raw whole-repo grep for `#[ignore]` also matches markdown prose — keep `--include='*.rs'`.

## Phase 2: Dimension Agents

Ordered by debt impact: complexity and duplication compound across every edit; doc/audit rot misdirects the *next*
audit; markers and dead code are cheap. Recent yield (`grep -h '\*\*Dimension\*\*' docs/audits/AUDIT_TECH_DEBT_*.md`):
Dims 4, 3, 1, 8 dominate (then 2, 9); 5 and 6 rarely surface real debt. Each agent writes `/tmp/audit/tech-debt/dim_<N>.md`.

### Dimension 1: File / Function / Module Complexity
Paths: all `.rs` under `crates/`, `byroredux/`, `tools/`
First step: the two-bucket command below

An oversized file taxes every edit, review and merge. Threshold is **2000 production LOC**. Two buckets:
```bash
# Primary — the dimension's actual subject. File real findings from this.
for f in $(find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*'); do
    p=$(prod_loc "$f"); [ "$p" -gt 2000 ] && echo -e "$p\t$f"
done | sort -rn
# Secondary — test-heavy files, report but do not auto-file: escalate one only if its OWN prod_loc also crosses 2000.
find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*' -exec wc -l {} + | awk '$1>2000 && $2!="total"' | sort -rn
```
The recipe output is authoritative. A file that once split can re-cross (a previously-split file that grew back is a
live finding); a file that dips under by incidental shrinkage is still a candidate — only an actual split removes one;
and files within ~5% of the line (check the snapshot) are worth naming as watch-list rows. Function-level splits inside
a file and file-level splits are independent signals — a `register_*_systems` / phase-helper extraction does not move
its host file across the line.

**Per file, propose a split AXIS by responsibility, chosen after reading the file's internal structure** — never from a
name or a tag. Precedent: a metadata enum tag (`ExtenderFamily`) touching 30 of 3759 lines was *not* the seam; the real
axis was the four-layer service stack (routes → declarations → source aliases → runtime adapters) with one service
holding over half the lines (#3851). Splits that worked: Vulkan `context/` and `volumetrics` by
construct-vs-record-vs-teardown; *boot.rs* into one file per registration stage; *runtime.rs* into one file per
`impl <wit>::Host` block. Data tables (FourCC → id maps, `shader_constants_data.rs`) want a static table or
generation, not a split. Vulkan-recording splits are render-pass-adjacent — read
*feedback_speculative_vulkan_fixes* before proposing barrier/order changes.

**Also flag**: functions >200 LOC (propose extraction); match arms >50 cases (want a lookup table); nesting depth >5;
a `mod.rs` / `lib.rs` with >20 `pub use` (two jobs). `cargo +nightly clippy --all-targets -- -W clippy::cognitive_complexity`
if available, else inspect the worst offenders.

### Dimension 2: Logic Duplication
Paths: sibling-file families under `crates/nif/src/blocks/`, `crates/plugin/src/esm/records/`, `crates/renderer/src/vulkan/`, `byroredux/src/cell_loader/`, `byroredux/src/systems/`
First step: `ls crates/nif/src/blocks/*.rs crates/plugin/src/esm/records/*.rs crates/plugin/src/esm/records/*/*.rs crates/renderer/src/vulkan/*.rs byroredux/src/cell_loader/*.rs`, then read two siblings side by side

CLAUDE.md global policy: *improve existing code, never duplicate logic.* Every finding names a concrete
consolidation site (an existing helper to extend, or the new one and its callers).
**Look for**: block-parser scaffolding (header → field → fixup) repeated across `crates/nif/src/blocks/`; texture-upload
chains (BC1/BC3/BC5/RGBA) and image-layout barrier sequences repeated per pass in `crates/renderer/src/vulkan/`
(`vulkan/image.rs` `GpuImage` and the shared barrier helpers are the consolidation targets — a pass that still
hand-rolls create/bind/destroy is a candidate); `vk::WriteDescriptorSet` builder boilerplate; ESM sub-record parse loops
across `crates/plugin/src/esm/records/`; AI-procedure systems sharing scaffolding; near-identical per-game HUD/profile
tables. The Z-up→Y-up conversion has one home, `crates/core/src/math/coord.rs` (`zup_to_yup_pos`,
`zup_to_yup_quat_wxyz`); re-exports elsewhere are not leaks — a genuine reimplementation would live outside that file.

### Dimension 3: Stale Documentation & Comments
Paths: `docs/`, `ROADMAP.md`, `HISTORY.md`, `README.md`, `CLAUDE.md`, doc comments across the tree
First step: `.claude/commands/_audit-validate.sh`

Doc rot misleads the next reader and the next audit. **Run the gate first**: `_audit-validate.sh` resolves every
backticked path in the audit skills AND `docs/engine/*.md` against the tree (fatal on STALE) and lists backticked
symbols found in no tracked `.rs` file (advisory — clear it, do not learn to scroll past it). STALE refs are
auto-eligible findings (trivial). Guards that already police a class — aim at what they cannot see:
- **`GpuMaterial` size**: `gpu_material_size_claims` (`crates/renderer/src/vulkan/material_tests.rs`) scans `crates`,
  `byroredux`, `tools`, `docs/engine`, `.claude/commands` and the top-level status docs for a stale `GpuMaterial` byte
  count (plus `bindings_glsl_states_the_real_struct_size`). It does NOT cover `GpuCamera` / `GpuInstance` / `Vertex::SIZE` or
  a size stated without the type name — cross-check those against the layout tests, never the prose:
  `grep -rn "fn gpu_.*_is_[0-9]\+_bytes\|size_of::<Gpu" crates/renderer/src/vulkan/`. A GPU-size claim found stale in
  prose is MEDIUM (severity table).
- **Deleted `Material::classify_pbr`**: `no_source_file_frames_the_deleted_classify_pbr_as_live`
  (`byroredux/src/workspace_hygiene_tests.rs`) fails a doc that names it as live. The survivors are the free fn
  `classify_pbr_keyword` and `Material::resolve_pbr`.
- Beyond the guards, sweep: doc comments naming renamed/deleted symbols (`git log --diff-filter=D --since=<last>` then
  grep each deleted name); ROADMAP.md milestones "in progress" whose issues are closed (or vice versa) — cross-check
  `git log` / `gh issue`; **`docs/feature-matrix.md`** rows against the crate that implements them (`git log --grep M45`,
  …) — it is a status floor, so flag any row contradicting shipped code; HISTORY.md entries referencing reverted work;
  README.md / `CLAUDE.md` command examples whose flags or paths changed; per-game compat numbers in `docs/engine/game-compatibility.md`
  vs ROADMAP.md's matrix (ROADMAP is the single home); `docs/legacy/` Gamebryo source paths that moved;
  `crates/renderer/shaders/triangle.frag` comments quoting outdated struct sizes.
- **`docs/engine/*.md` are the "authoritative" references audits are told to believe** (`_audit-common.md`): beyond the gate's
  path/symbol check, sample their numbers and lists against code — stage order and lock order (`ecs.md`), VRAM/pool figures
  (`memory-budget.md`), shader binding tables (`shader-pipeline.md`), dependency and crate lists — a doc that misled an audit
  in the last 90 days is MEDIUM.
- **Path convention**: a backticked path or snake_case symbol in an audit skill asserts "exists right now"; deleted or
  forward-looking names go in italics or plain text (`_audit-common.md` Path-Reference Convention).

### Dimension 4: Audit-Finding Rot
Paths: `.claude/commands/`, `docs/audits/`
First step: `.claude/commands/_audit-validate.sh` then `ls .claude/commands/ docs/audits/ | tail -40`

Stale baselines actively misdirect future audits.
- STALE refs the gate prints in *other* audit skills → Dim 4 findings (trivial); advisory symbols in skills likewise.
- Numeric claims in skills and `_audit-common.md` (LOC figures, struct sizes, counts, "all N dimensions") — the gate
  cannot see numbers: re-measure a sample (`wc -l`, *prod_loc*, `grep -c`) and flag any off by >20%. Layout-row LOC
  drift has been the single most repeated finding here.
- "Existing: #NNN" / "open issue" callouts in skills where the issue is now CLOSED: `gh issue view N --json state`.
- Dimension cross-references between skills ("`/audit-x` Dim N") that no longer name that dimension.
- `docs/audits/` reports older than 90 days whose CRITICAL/HIGH findings have no GitHub trace. Known-open: reports
  dated before 2026-06-07 predate `/audit-publish`, so a missing issue is *not* evidence a finding is open (#3875,
  `_audit-common.md` Deduplication) — verify against code and say which you checked.
- Do NOT flag `.claude/issues/<N>/ISSUE.md` "Status: Open" drift: local issue files are immutable snapshots (TD10-001 / #1156);
  `gh issue view <N> --json state` is authoritative.

### Dimension 5: Stale Markers (TODO / FIXME / HACK / XXX / TBD)
Paths: `crates/`, `byroredux/`, `tools/` (not `tools/nifskope`), `crates/renderer/shaders/`
First step: the two greps below
```bash
grep -RInE '(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b' crates byroredux tools --exclude-dir=nifskope
grep -RInE '(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b' crates/renderer/shaders/
```
Keep the token list identical in both commands (an asymmetry once hid every shader `FIXME`, #3877); `TBD` is in it because
the codebase uses it. `tools/` (`byro-dbg`, `byro-launcher`, `byro-detect`, `texture-upscale`) is first-party and in scope.
**Triage each** (skip markers <30 days old unless they name a closed issue): `git blame` for age — over 6 months gets
reported; does it name an issue, and is that issue still open (closed issue + live marker = "marker outlived its driver");
does it name a milestone now complete per ROADMAP.md; a `// TODO: implement` on a path reachable from a shipped CLI flag
promotes (severity table).
**False positives**: `XXXX`, the ESM extended-size sub-record tag (key on comment content — "references the ESM `XXXX`
escape" — not a file list; `reader.rs`, `cell/wrld.rs`, the `b"XXXX"` test sentinels in `records/misc/magic.rs` and the
FourCC table doc in `sdk/src/compatibility/storage_util/mod.rs` all qualify); a `// FIXME` quoting a reference implementation's
own FIXME (`crates/bgsm/src/bgem.rs`, `bs_geometry.rs`) documents upstream; a `TBD` that records an unresolved format
semantic together with its own resolution (the FNV `WEAP` `DNAM` arm in `crates/plugin/src/esm/records/items.rs`) is a
documented unknown, not a stale marker.
**Must-not-delete**: the third-party attribution block atop `crates/renderer/shaders/triangle.frag` (GLSL-PathTracer MIT
notice + Burley citation, ~first 30 lines). Flag any edit that strips it — MIT requires the notice to travel with the code.

### Dimension 6: Stub & Placeholder Implementations
Paths: `crates/`, `byroredux/`, `tools/`
First step: the two greps below
```bash
grep -RInE 'unimplemented!|todo!\(\)|panic!\("not ' crates byroredux tools --exclude-dir=nifskope
grep -RInE '// *(stub|TODO: real|placeholder|not yet)' crates byroredux tools --exclude-dir=nifskope
```
The first command should return nothing — any hit is notable. For each: reachable from a shipped CLI flag or smoke test →
MEDIUM; functions returning `None` / `Vec::new()` / `Default::default()` under a stub comment; trait impls with empty bodies
the trait docs say should do work; console commands in `byroredux/src/commands/` that no-op or print "TODO".
**Production-unreachable features** are the yield here: a public builder/system with zero non-test callers whose docs or
ROADMAP say it is wired (a ruleset builder that `main` never calls; a component nothing inserts). For a candidate
`name`: `grep -rnw name crates byroredux tools --include='*.rs'` and drop test-only hits. Cross-check per-game ESM record
coverage in `crates/plugin/src/esm/records/` against the ROADMAP.md compat matrix (wired vs stubbed).

### Dimension 7: Magic Numbers & Hardcoded Constants
Paths: `crates/nif/src/blocks/`, `crates/renderer/`, `crates/plugin/src/esm/records/`
First step: read the version-gate and budget sites; do not regex blindly (most literals are legitimate)

- Bare numeric literals compared against version codes in `crates/nif/src/blocks/` → a `NifVersion` constant.
- Vulkan `MAX_*` / `MIN_*` hardcoded inline → `vk::PhysicalDeviceLimits` or a named constant.
- **Shader `#define` provenance**: every define is generated from `crates/renderer/src/shader_constants_data.rs`
  (`include!`d by `shader_constants.rs` and `crates/renderer/build.rs`, which emits `shaders/include/shader_constants.glsl`);
  source-scan tests in `shader_constants.rs` police the generated header. The check is "every shader `#define` / loop
  bound / budget literal that bypasses that file" (lockstep risk HIGH — *feedback_shader_struct_sync*).
- GPU `#[repr(C)]` size literals: reference the layout pins (Dim 3 has the recipe and the guards); an inline size
  literal that should reference them is a code finding here, a stale prose figure is Dim 3.
- Frame/ray/cache budgets (`MAX_TOTAL_BONES`, `MAX_MATERIALS`, …) scattered vs one tunable module; ESM sub-record sizes
  hardcoded (`if data.len() == 24`) → a named constant on the record struct.
- Do NOT flag protocol-defined magic: FourCC tags, BSA/NIF/BA2 magic, Vulkan format enums.

### Dimension 8: Dead Code & Backwards-Compat Cruft
Paths: `crates/`, `byroredux/`, `tools/`, every `Cargo.toml`
First step: `cargo clippy --workspace -- -D warnings` (the CI gate) and the greps below
```bash
grep -RInE 'allow\(dead_code\)' crates byroredux tools --exclude-dir=nifskope
grep -RInE '#\[deprecated\]|// *removed:|_unused|fn .*_unused' crates byroredux
cargo machete 2>/dev/null || echo "cargo machete not installed — scan Cargo.toml deps vs use stmts"
```
- **Clippy gate** (CI job `Test + Check + Clippy` runs `cargo clippy --workspace -- -D warnings`; the toolchain is stable, so
  a new rustc/clippy raises lints on untouched code — rustc 1.96 did, commit 800802516). A red gate is a finding: list each
  failing lint and file. Run `--all-targets --keep-going` too (the plain form aborts at the first failing crate);
  test/example-target lints are outside the CI gate — report them as a lower-priority bucket. Check every
  `#[allow(clippy::…)]` carries a reason comment (the house style for `too_many_arguments`).
- Each `#[allow(dead_code)]`: called now or still dead? Delete if dead. `pub fn` in a private module nobody imports (`cargo +nightly rustc -p <crate> -- -W unused`);
  `mod.rs`/`lib.rs` re-exports with no consumer; `_`-prefixed params that survived a refactor (delete, do not rename);
  `// removed: …` breadcrumbs (delete completely); re-exports of deleted types "for compatibility" (no external consumers
  exist — pure rot); `#[deprecated]` items with no consumers; `Cargo.toml` feature flags with one branch always on/off.
- **Do NOT flag**: `cfg(test)` / `cfg(debug_assertions)`-gated code, FFI boundary functions, or the public API of a
  workspace-internal crate a future binary will consume (note it rather than deleting).

### Dimension 9: Test Hygiene
Paths: every `#[test]`/`#[ignore]` site, `.github/workflows/ci.yml`, `byroredux/tests/`
First step: the ignore-triage command below
```bash
# All ignores (matches BOTH `#[ignore]` and the reason form `#[ignore = "…"]` — do not tighten to a bare `]`)
grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools
# Ignores whose reason is NOT a data/hardware gate — the triage input
grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools | grep -viE 'game data|installed|\.(bsa|ba2|esm)|BSA|BA2|ESM|audio device|vulkan|gpu|rt-capable|display|opt-in|corpus|real (data|master)|steam|env'
```
- **Guards**: `workspace_hygiene_tests` (`byroredux/src/workspace_hygiene_tests.rs`) — `every_ignore_attribute_carries_a_reason`
  (a bare `#[ignore]` fails; the reason text is the triage input), `no_tmp_scratch_examples_are_committed` (`_tmp_*`
  probes), and the `classify_pbr` check (Dim 3). Confirm they are not themselves `#[ignore]`d.
- Most `#[ignore]`s gate Vulkan / game-data / audio tests — not debt. Triage the remainder: does a reason name an issue
  (`gh issue view N`), and if it guards a closed CRITICAL/HIGH fix → MEDIUM.
- Tests with only smoke assertions (`assert!(result.is_ok())`); commented-out assertions in passing tests; tests that
  `println!` without an assert.
- **Feature gates vs CI lanes**: enumerate `[features]` (`grep -A6 '^\[features\]' crates/*/Cargo.toml byroredux/Cargo.toml`)
  and compare with `.github/workflows/ci.yml` — non-default states run only where a lane names them (today: `core`
  `inspect`/`save` via workspace unification, `byroredux-spt --features recon`, `byroredux` without `debug-server` / with
  `tracing-tracy` / with `dhat-heap`, `byroredux-nif --features dhat-heap`). A feature or `required-features` target with no
  lane can rot silently (the class #3894 / #4387 / #4390 closed).
- `byroredux/tests/golden_frames.rs` (opts into `--ignored`) — still runnable, golden images current.
- Cross-reference "must not regress" lines in other audit skills — each named regression test still exists and is not
  `#[ignore]`d (`_audit-validate.sh` advisory names symbols that are gone).

## Cross-Dimension Dedup

A TODO inside a dead function reports under Dim 8, not also Dim 5. Material doc rot reports under Dim 3, not Dim 8. A stale
GPU-size doc comment is Dim 3; a stale GPU-size *code literal* is Dim 7. NIFAL/material *translation correctness* is out of
scope (`/audit-nifal`) — this audit owns only the debt around that tier.

## Phase 3: Merge

1. Read all `/tmp/audit/tech-debt/dim_*.md`.
2. Combine into `docs/audits/AUDIT_TECH_DEBT_<TODAY>.md`: **Executive Summary** (findings by severity + delta vs
   `baseline.txt`), **Baseline Snapshot** (the Phase-1 counts), **Top 10 Quick Wins** (trivial/small effort),
   **Top 5 Medium Investments** (splits, consolidations), **Findings** (HIGH → MEDIUM → LOW, then by dimension),
   **Deferred** (gated on an in-progress milestone; name it).
3. Remove cross-dimension duplicates per the rules above.

## Phase 4: Cleanup

`rm -rf /tmp/audit/tech-debt`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_TECH_DEBT_<TODAY>.md`.

## GitHub Labels

Findings publish under `tech-debt` (plus the standard `<severity>` and `<domain>` labels; `/audit-publish` applies it for
`TECH_DEBT` reports). Two sibling kind labels split the bucket — apply the one that matches:
- **`doc-rot`** — documentation drifted from code (stale ROADMAP row, a SKILL naming a deleted symbol, a comment describing
  removed behaviour); publishes as `documentation`, not `bug`.
- **`test-gap`** — missing, vacuous or non-asserting coverage (an `#[ignore]`d test with no data gate, an entry point with
  zero tests).
Pure debt (dead code, duplication, magic numbers, oversized files, stale markers) stays `tech-debt` + `bug`.
