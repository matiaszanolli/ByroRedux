---
description: "Delta audit — check only recently changed code for regressions and new bugs"
argument-hint: "--commits <N> --since <date>"
---

# Incremental / Delta Audit

Audit only code that has changed recently. Faster than a full audit, focused on new regressions.

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--commits <N>`: Number of recent commits to audit (default: 10)
- `--since <date>`: Audit changes since this date (e.g., `2026-04-01`). Overrides `--commits`.

## Step 1: Gather Changed Files

```bash
git log --oneline -10
git diff HEAD~10..HEAD --name-only
```

If `--since` provided: `git log --since="<date>" --oneline && git diff $(git log --since="<date>" --format="%H" | tail -1)..HEAD --name-only`

## Step 2: Categorize Changes by Risk

| Domain | File Patterns | Risk |
|--------|--------------|------|
| **Vulkan/GPU** | `crates/renderer/src/vulkan/**` | HIGH |
| **RT / Accel** | `crates/renderer/src/vulkan/acceleration/`, `svgf.rs`, `gbuffer.rs`, `composite.rs` | HIGH |
| **Volumetrics (M55)** | `crates/renderer/src/vulkan/volumetrics.rs`, `crates/renderer/shaders/volumetric_*.comp` | HIGH |
| **Bloom (M58)** | `crates/renderer/src/vulkan/bloom.rs`, `crates/renderer/shaders/bloom_*.comp` | HIGH |
| **Water (M38)** | `crates/renderer/src/vulkan/water.rs`, `crates/renderer/shaders/water.{vert,frag}`, `byroredux/src/systems/water.rs`, `byroredux/src/cell_loader/water.rs` | HIGH |
| **Shaders** | `crates/renderer/shaders/**` | HIGH |
| **ECS Core** | `crates/core/src/ecs/**` | HIGH |
| **NIF Parser** | `crates/nif/src/blocks/**`, `crates/nif/src/import/**` | HIGH |
| **BSA/Archive** | `crates/bsa/src/**` | HIGH |
| **SpeedTree** | `crates/spt/src/**`, `byroredux/src/cell_loader/refr.rs` (.spt route) | MEDIUM |
| **ESM Parser** | `crates/plugin/src/esm/**` (incl. `records/misc/**` post-Session-34 split: water/character/world/ai/magic/effects/equipment) | MEDIUM |
| **Animation** | `crates/core/src/animation/**`, `crates/nif/src/anim/` (post-Session-35 split: `entry`, `sequence`, `controlled_block`, `transform`, `bspline`, `channel`, `keys`, `coord`, plus `types.rs` and `tests.rs`) | MEDIUM |
| **Cell Loader** | `byroredux/src/cell_loader/**` (was monolithic `cell_loader.rs` pre-Session-34) | MEDIUM |
| **Systems** | `byroredux/src/systems/**` (was monolithic `systems.rs` pre-Session-34) + `byroredux/src/render.rs` + `render/**` | MEDIUM |
| **Scene Setup** | `byroredux/src/scene/**` (was monolithic `scene.rs` pre-Session-34) | MEDIUM |
| **Main Loop** | `byroredux/src/main.rs`, `byroredux/src/commands.rs` | MEDIUM |
| **Asset Provider** | `byroredux/src/asset_provider.rs` (sibling-BSA auto-load, AE pipeline-path strip) | MEDIUM |
| **Audio** | `crates/audio/src/{lib,tests}.rs` | MEDIUM |
| **Tests** | `**/tests/**`, `**/*_tests.rs`, `byroredux/tests/golden_frames.rs` | LOW |
| **Docs** | `*.md`, `docs/**` | LOW |

## Step 3: Audit Each Changed File

For each changed file, read the diff and surrounding context. Check:

- [ ] **New bugs**: Logic errors, off-by-ones, wrong byte sizes, missing version checks?
- [ ] **Unsafe changes**: New unsafe blocks? Changed safety invariants? Missing safety comments?
- [ ] **Lock scope**: Changed RwLock acquisition? New query patterns? Potential deadlocks?
- [ ] **Vulkan correctness**: New pipeline/barrier/sync changes? Missing validation? RT acceleration structure changes?
- [ ] **NIF correctness**: New block parsers consume correct byte count? Version conditionals right?
- [ ] **Tests**: Corresponding test updates? New code paths tested?
- [ ] **Contract breaks**: Public API changed — did ALL callers update?

## Step 4: Rust-Specific Checks

For Rust files specifically:
- [ ] **Lifetime changes**: Borrowed references changed scope? Temporary lifetimes?
- [ ] **Drop ordering**: Vulkan object destruction order still correct?
- [ ] **Error handling**: New `unwrap()` calls? Changed error propagation?
- [ ] **Trait impls**: New trait implementations consistent with existing ones?

## Extra Per-Finding Fields

- **Changed File**: `<file-path>` (commit: `<hash>`)

## Output

Write to: **`docs/audits/AUDIT_INCREMENTAL_<TODAY>.md`**

### Report Structure
1. **Change Summary** — Files changed, commit range, key themes
2. **High-Risk Changes** — Files in Vulkan/ECS/NIF/shader domains
3. **Findings** — New bugs, regressions, or gaps
4. **Missing Tests** — Changed code paths without test updates

Suggest: `/audit-publish docs/audits/AUDIT_INCREMENTAL_<TODAY>.md`
