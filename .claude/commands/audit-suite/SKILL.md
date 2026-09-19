---
description: "Run a preset suite of audits in parallel — or derive one from a code area / diff via the ownership map"
argument-hint: "--preset <name> | --area <path…> | --changed <git-scope>"
---

# Audit Suite Orchestrator

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

This skill owns no audit logic: it **selects** audits and fans them out as background agents, then merges their reports. Audits are `.claude/commands/audit-<name>/SKILL.md`, invoked `/audit-<name>`. `/audit-publish` is post-processing and never appears in a suite. Who owns which path: `.claude/commands/_audit-owners.md`.

## Modes

1. **`--preset <name>`** — a curated list (below).
2. **`--area <path…>`** — derive the set: `git ls-files <path…> | .claude/commands/_audit-route.sh` → owners of those files, plus the *neighbors* of each owner from the table below. No hand-maintained preset to drift.
3. **`--changed <scope>`** — same, from `git diff <scope> --name-only` (scope forms as in `/audit-incremental`). Fans out whole owner audits; use `/audit-incremental` when you only want hunk-level checks.

Each audit delta-scopes itself (`_audit-common.md` § Delta-first scoping), so a suite over a code area costs roughly what changed there. Pass `--focus <dimensions>` to one audit only when the user names it; never bake dimension numbers into a preset — they drift on every renumbering.

## Curated presets

| Preset | When | Audits |
|---|---|---|
| `quick` | after any change, <10 min | `incremental --commits 5` |
| `pre-release` | before tagging | safety · renderer · ecs · `regression --limit 20` |
| `comprehensive` | monthly / pre-milestone | every audit below, then `runtime --game all` |
| `tech-debt-deep` | after a milestone closes | tech-debt · `incremental --commits 30` |
| `per-game-all` | compat sweep, reference title first | fnv · fo3 · skyrim · oblivion · fo4 · starfield |
| `nif-all-games` | NIF parser vs every corpus | `nif --game` each of fnv fo3 skyrim oblivion fo4 starfield |
| `runtime-regression` | telemetry vs baselines | `runtime --game all` |

`comprehensive` covers every owner in `_audit-owners.md`: renderer · performance · ecs · concurrency · safety · nif · nifal · esm · parsers · physics · character · gameplay · exterior · ui · audio · speedtree · scripting · papyrus · save · tooling · legacy-compat · tech-debt · the six per-game audits · regression. If an owner row names an audit missing from this list, add it here — `_audit-validate.sh` fails on owners that have no skill, and reading the map is the coverage check.

## Neighbors (added by `--area` / `--changed`)

| Owner | Always add |
|---|---|
| renderer | performance · concurrency · safety |
| nif | nifal · safety |
| nifal | nif · renderer |
| esm | the per-game audit, only when a title is the target |
| physics | concurrency · safety |
| character | esm · ecs |
| gameplay | ecs · save · physics |
| exterior | renderer · physics (water half) · esm (WATR/WTHR/GRAS decode) |
| parsers | safety · the format's per-game audit |
| audio | concurrency · safety |
| ui | safety · concurrency · tech-debt |
| scripting | ecs · papyrus (when `.pex`/`.psc` inputs are involved) |
| papyrus | scripting · safety (decompiler recursion/panic bounds) |
| save | ecs · gameplay |
| tooling | concurrency (debug-server threads) · safety |

## Aliases for the old `*-deep` presets

Each is just `--area` over these paths (neighbors apply):

| Alias | `--area` paths |
|---|---|
| `renderer-deep` | `crates/renderer/` `byroredux/src/render/` |
| `rt-deep` | `crates/renderer/src/vulkan/acceleration/` `crates/renderer/src/vulkan/{svgf,gbuffer,composite}.rs` |
| `material-deep` | `crates/renderer/src/vulkan/material.rs` `crates/renderer/src/vulkan/scene_buffer/` `byroredux/src/material_translate.rs` |
| `texture-roles-deep` | `crates/nif/src/import/material/` `byroredux/src/asset_provider/material/` `crates/bgsm/` `crates/sfmaterial/` |
| `upscaler-deep` | `crates/fsr3-sys/` `crates/renderer/src/vulkan/{frame_upscaler,upscaling,presentation,exposure}.rs` (run with `BYRO_VALIDATION=1`) |
| `water-deep` | `crates/renderer/src/vulkan/water.rs` `crates/physics/` `byroredux/src/cell_loader/water.rs` `byroredux/src/systems/water.rs` |
| `volumetrics-deep` / `bloom-deep` / `skin-deep` | `crates/renderer/src/vulkan/volumetrics.rs` / `bloom.rs` / `skin_compute.rs` + `acceleration/blas_skinned.rs` |
| `nif-deep` / `nifal-deep` | `crates/nif/` / `byroredux/src/material_translate.rs` `crates/nif/src/import/` |
| `esm-deep` / `character-deep` / `physics-deep` | `crates/plugin/` / `crates/core/src/character/` / `crates/physics/` `byroredux/src/ragdoll.rs` |
| `ui-deep` / `audio-deep` / `save-deep` | `crates/ui/` `crates/menuxml/` `byroredux/src/hud.rs` / `crates/audio/` / `crates/save/` `byroredux/src/save_io/` |
| `scripting-deep` / `speedtree-deep` | `crates/{scripting,pex,papyrus}/` / `crates/spt/` |
| `streaming-deep` | `byroredux/src/streaming.rs` `byroredux/src/npc_spawn/` `byroredux/src/cell_loader/` |
| `exterior-deep` / `gameplay-deep` / `parsers-deep` / `tooling-deep` | the new owners' `Paths:` lines |
| `legacy-deep` | `legacy-compat` alone |

## Execution

1. Parse the mode. Unknown preset → list the tables above and stop. `mkdir -p /tmp/audit`.
2. Launch each audit as a **background agent**, max 3 concurrent — they read the tree and write distinct reports, so there is no ordering dependency.
3. **Orchestration hazard**: an agent that fans out its own sub-agents cannot receive their completion notices and will drop real findings from its report (seen on the 2026-08-03 `comprehensive` run: three real findings — incl. a HIGH ragdoll double-compose — were reported "clean"). Tell every launched audit up front to analyse dimensions synchronously or to write `/tmp/audit/<name>/dim_N.md` scratch files. Before accepting "no findings" for a dimension, check that scratch file against the written report.
4. Each audit writes `docs/audits/AUDIT_<TYPE>_<TODAY>.md` (`_audit-common.md` § Report Finalization). Verify each exists with real content, not a placeholder.
5. Merge into a summary; warn prominently if any CRITICAL:

```markdown
# Audit Suite Summary — <mode + args> — <date>

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|---|---|---|---|---|---|---|

Total: X findings (C critical, H high, M medium, L low). Unchanged-since-baseline dimensions skimmed: <list>
```

6. For each report with findings suggest `/audit-publish docs/audits/AUDIT_<TYPE>_<TODAY>.md`.
