## Description

Session 36 (today, commits 29e9f45..ca81c19) split 7 monolith files into per-topic directories:

- `crates/renderer/src/vulkan/acceleration.rs` → `acceleration/`
- `crates/renderer/src/vulkan/scene_buffer.rs` → `scene_buffer/`
- `crates/nif/src/blocks/collision.rs` → `blocks/collision/`
- `crates/nif/src/anim.rs` → `anim/`
- `crates/nif/src/import/mesh.rs` → `import/mesh/`
- `crates/nif/src/blocks/dispatch_tests.rs` → `blocks/dispatch_tests/`
- `crates/plugin/src/esm/cell/tests.rs` → `cell/tests/`

The doc-refresh commit `5ab6a8b` updated CLAUDE.md / HISTORY.md / ROADMAP.md / docs/engine/*.md — but did **NOT** touch any `.claude/commands/audit-*.md` skill files. Every audit run grepping for those paths now hard-fails on stale grep targets.

Verified: `grep -nE "acceleration\.rs|scene_buffer\.rs|anim\.rs|import/mesh\.rs|blocks/collision\.rs" .claude/commands/_audit-common.md .claude/commands/audit-*.md | wc -l` → **29 hits** as of 2026-05-14.

This batch consolidates the Dim 7 (Stale Documentation) + Dim 10 (Audit-Finding Rot) findings from `AUDIT_TECH_DEBT_2026-05-14.md`.

## Findings consolidated

### TD7-025..033 — `_audit-common.md` Project Layout pins deleted files
- **File**: `.claude/commands/_audit-common.md:16-43`
- 9 paths still cite the pre-Session-36 files:
  - L16 `NIF Animation: crates/nif/src/anim.rs + anim/{types.rs, tests.rs}` → needs `anim/{coord, controlled_block, transform, sequence, keys, channel, bspline, entry, mod, types, tests}.rs`
  - L30 `Accel (RT): crates/renderer/src/vulkan/acceleration.rs` → directory of 9 submodules
  - L43 `Scene Buffers: crates/renderer/src/vulkan/scene_buffer.rs` → directory of 5 prod + 3 test submodules
  - Plus `import/mesh.rs`, `blocks/collision.rs`, `blocks/dispatch_tests.rs`, `cell/tests.rs` references in the same Project Layout block
- **Fix**: replace each `.rs` ref with `/` and inline the submodule list. Single batched sed pass.

### TD7-034..038 — `audit-*.md` skill files cite split files in must-not-regress anchors
- Affected files: `audit-renderer.md`, `audit-performance.md`, `audit-concurrency.md`, `audit-nif.md`, `audit-safety.md`
- Each cites at least one Session-36-split file by `.rs` name in a "must not regress" anchor line.
- **Fix**: convert to symbol-based references per `#1040`'s pattern (already proven across 5 skill files).

### TD7-039 — `audit-renderer.md:241,252` says GpuInstance lives in 3 shaders; actual is 5
- **File**: `.claude/commands/audit-renderer.md:241,252`
- Actual count via `grep -l "struct GpuInstance" crates/renderer/shaders/`: **5** (`triangle.vert`, `triangle.frag`, `ui.vert`, `caustic_splat.comp`, `water.vert`)
- The memory note `feedback_shader_struct_sync.md` says 4 — also wrong.
- Prior audit's TD10-002 said 6 — also wrong (closeout used the wrong count).
- **Fix**: update both files to the verified count of 5; add a `grep -l "struct GpuInstance" shaders/*` invocation to the skill as the canonical drift check.

### TD10-013 — `audit-renderer.md:282` + `audit-safety.md:76` carry stale DBG_* bit anchors
- DBG_* bits cited at `triangle.frag:628-686`; actual range is **718-780** (~90-line drift).
- Still lists `DBG_FORCE_NORMAL_MAP = 0x20`; **#1035** (closed 2026-05-14T18:03) renamed it to `DBG_RESERVED_20`.
- Closeout-rot specific to today's #1035 commit chain.
- **Fix**: bump line range and the bit name in both skill files.

### TD7-041 — ROADMAP claims `#687 / #688 / #697 / #698` are "open tracking issues"; all 4 CLOSED
- 6 sites in ROADMAP.md mention these as open trackers; all 4 issues are CLOSED on GitHub per `gh issue view`.
- **Fix**: repoint to "tracked under [git log]" or remove the active-tracker framing.

### TD7-042 — `HISTORY.md:170` says `MAX_MATERIALS = 1024`; actual is 4096
- Constant lives at `crates/renderer/src/vulkan/scene_buffer/constants.rs:103` and is `4096`.
- HISTORY is append-only but this is a typo in the still-relevant Session-32 entry.
- **Fix**: one-line bump.

### TD7-044 — `scene_buffer/gpu_types.rs:158-161` mid-edit narrative in `///` docstring
- "wait — six trailing vec4s" thinking-aloud artifact in a doc comment.
- Math is correct; narrative needs cleanup.

## Completeness Checks

- [ ] **UNSAFE**: N/A — docs-only changes
- [ ] **SIBLING**: After fixing `_audit-common.md`, re-grep every `audit-*.md` skill for residual `.rs` refs to the 7 split files. Run `grep -rnE "acceleration\.rs|scene_buffer\.rs|anim\.rs|import/mesh\.rs|blocks/collision\.rs|cell/tests\.rs|dispatch_tests\.rs" .claude/commands/` and confirm zero hits.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A (docs); but adding a `grep -L` pre-commit check that fails on stale paths is worth considering for the long term — fold into the #1040 followup pattern.

## Effort
small (≤2 h, one batched PR with sed for the trivial path bumps + manual symbol-based rewrites where line numbers drift)

## Cross-refs

- Audit report: `docs/audits/AUDIT_TECH_DEBT_2026-05-14.md`
- Prior #1040 (audit-skill anchor rot, CLOSED) — same flavor but for Session-34-era drift
- The session35_layout / session36_layout memory notes are the translation key the audit skills should reference
