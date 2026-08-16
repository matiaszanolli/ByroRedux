---
description: "Run a preset suite of audits in parallel"
argument-hint: "--preset <name>"
---

# Audit Suite Orchestrator

Runs a **named preset** — a curated list of other `/audit-*` skills — by fanning
them out as background agents in parallel, then merges their reports into one
summary. This skill owns no audit logic of its own; it only sequences and
aggregates the individual audits. Shared protocol (project layout, severity,
dedup, report format) lives in `.claude/commands/_audit-common.md` and
`.claude/commands/_audit-severity.md` — not repeated here.

Every audit referenced below is a live skill under
`.claude/commands/audit-<name>/SKILL.md`, invoked as `/audit-<name>`.
The full current set (27): audio, character, concurrency, ecs, esm, fnv, fo3,
fo4, incremental, legacy-compat, nif, nifal, oblivion, performance, physics,
publish, regression, renderer, runtime, safety, save, scripting, skyrim,
speedtree, starfield, tech-debt, ui.
(`/audit-publish` is a post-processing step, not an analysis pass — it never
appears in a preset. `/audit-scripting` owns crates/scripting + crates/pex +
crates/papyrus; `/audit-save` owns crates/save — both added 2026-06-23.
**Four owner audits added 2026-08-13**, closing the largest coverage gaps:
`/audit-esm` owns crates/plugin, `/audit-ui` owns crates/ui, `/audit-physics`
owns crates/physics + byroredux/src/ragdoll.rs, `/audit-character` owns
crates/core/src/character. Presets that used to substitute generic audits for
those areas now call the owner directly.)

**`--focus` numbers below track the dimension numbering inside each target skill.
If a target audit is renumbered, update the focus lists here in lockstep** — the
suite is the one place those numbers are duplicated, so it drifts first.

## Preset Index

| Preset | When | Audits |
|--------|------|--------|
| `quick` | after any change, < 10 min | incremental |
| `pre-release` | before tagging | safety · renderer · ecs · regression |
| `comprehensive` | monthly / pre-milestone | all subsystem + per-game + runtime |
| `tech-debt-deep` | after a milestone closes | tech-debt · incremental |
| `per-game-all` | per-game compat sweep | the 6 game audits |
| `nif-all-games` | NIF parser vs every game | nif ×6 game corpora |
| `runtime-regression` | telemetry diff vs baselines | runtime |
| `esm-deep` | after ESM/plugin-parser changes | esm · incremental · (per-game when one title is the target) |
| `physics-deep` | after physics / ragdoll / character-controller changes | physics · concurrency · safety |
| `character-deep` | after CHARAL ruleset / progression changes | character · esm · ecs |
| `nif-deep` | after NIF parser changes | nif · nifal · safety · incremental |
| `nifal-deep` | after NIFAL translation changes | nifal · nif · renderer · ecs |
| `renderer-deep` | after renderer changes | renderer · performance · concurrency · safety |
| `rt-deep` | after RT / denoiser / G-buffer changes | renderer · performance · concurrency |
| `material-deep` | after material-table / PBR changes | renderer · safety |
| `texture-roles-deep` | after `MaterialTextureSet` / `ImportedMaterial` changes | nifal · fo4 · starfield · renderer |
| `upscaler-deep` | after FSR3 / presentation / exposure changes | renderer · safety · performance |
| `ui-deep` | after Scaleform/SWF (R4 + M48) changes | ui · safety · concurrency · tech-debt |
| `water-deep` | after water changes (render **or** physics half) | renderer · physics · esm · concurrency · safety |
| `volumetrics-deep` | after volumetric-lighting changes | renderer · performance · safety |
| `bloom-deep` | after bloom-pyramid changes | renderer · performance · safety |
| `skin-deep` | after GPU-skinning / BLAS-refit changes | renderer · performance · concurrency · safety |
| `audio-deep` | after audio (kira) changes | audio · concurrency · safety |
| `scripting-deep` | after scripting / .pex / Papyrus / recognizer changes | scripting · ecs · incremental |
| `save-deep` | after save/load changes | save · ecs · incremental |
| `speedtree-deep` | after SpeedTree (.spt) changes | speedtree · incremental |
| `streaming-deep` | after world-streaming / NPC-spawn changes | performance · concurrency · character · physics · safety |
| `legacy-deep` | after compatibility-mapping work | legacy-compat · incremental |

## Broad Presets

### `--preset quick`
Fast sanity check after a change (< 10 min):
1. `/audit-incremental --commits 5`

### `--preset pre-release`
Run before tagging a release:
1. `/audit-safety`
2. `/audit-renderer`
3. `/audit-ecs`
4. `/audit-regression --limit 20`

### `--preset comprehensive`
Full coverage (longest — run monthly or before a major milestone). Every
subsystem audit, every per-game audit, plus the runtime telemetry diff that
catches what static audits structurally can't see:
1. `/audit-renderer`
2. `/audit-ecs`
3. `/audit-safety`
4. `/audit-concurrency`
5. `/audit-performance`
6. `/audit-nif`
7. `/audit-nifal`
8. `/audit-esm`
9. `/audit-physics`
10. `/audit-character`
11. `/audit-ui`
12. `/audit-audio`
13. `/audit-speedtree`
14. `/audit-scripting`
15. `/audit-save`
16. `/audit-legacy-compat`
17. `/audit-tech-debt`
18. `/audit-fnv`
19. `/audit-fo3`
20. `/audit-skyrim`
21. `/audit-oblivion`
22. `/audit-fo4`
23. `/audit-starfield`
24. `/audit-regression`
25. `/audit-runtime --game all`

With the four 2026-08-13 additions this preset covers every crate in the
`_audit-common.md` crate→owner map. It does **not** cover the six un-owned
subsystems in that file's "Un-owned subsystems" table (refreshed 2026-08-16) —
name them in the summary rather than claiming full coverage:

- **the P2 gameplay slice** (`byroredux/src/{combat,inventory,settings_io}.rs` +
  the action half of `interaction.rs`) — no owner, and the project's active
  execution focus. If this preset is run to bless a release, run `/audit-ecs`
  and `/audit-runtime` with these files explicitly in scope first.
- `crates/facegen` (incidental to `/audit-skyrim` only)
- `crates/mod-runtime` (folded into `/audit-safety` Dim 11)
- `crates/hkx` (folded into `/audit-scripting` Dim 8)
- `crates/debug-server` / `crates/debug-protocol` (no owner)
- `crates/fsr3-sys` + the upscaler passes (folded into `/audit-renderer` Dim 23)

### `--preset tech-debt-deep`
Surface accumulated debt (run after a milestone closes, before opening the next):
1. `/audit-tech-debt`
2. `/audit-incremental --commits 30`

## Per-Game Presets

### `--preset per-game-all`
Run every per-game compatibility audit (reference title first, then in
compat-tier order):
1. `/audit-fnv`
2. `/audit-fo3`
3. `/audit-skyrim`
4. `/audit-oblivion`
5. `/audit-fo4`
6. `/audit-starfield`

### `--preset nif-all-games`
Exercise the NIF parser against every available game corpus (the `--game` arm
selects the on-disk data dir from `_audit-common.md`):
1. `/audit-nif --game fnv`
2. `/audit-nif --game fo3`
3. `/audit-nif --game skyrim`
4. `/audit-nif --game oblivion`
5. `/audit-nif --game fo4`
6. `/audit-nif --game starfield`   # Cydonia walkable — BSGeometry path exercised

### `--preset runtime-regression`
Drive the engine headless on every supported game's representative cell and diff
the captured telemetry against the checked-in baseline TSVs. Catches regressions
in `tex.missing` / `mesh.cache failed` / fps / draw-call count under a real cell
load — see [#1283](https://github.com/matiaszanolli/ByroRedux/issues/1283):
1. `/audit-runtime --game all`

## NIF / NIFAL Presets

### `--preset nif-deep`
After NIF parser changes (stream position, version gating, block dispatch):
1. `/audit-nif`
2. `/audit-nifal`           # the parse → ECS material/collision boundary regresses with parser changes
3. `/audit-safety`
4. `/audit-incremental --commits 10`

### `--preset nifal-deep`
After NIFAL canonical-translation changes — the single `ImportedMesh` → `Material`
boundary (`byroredux/src/material_translate.rs`), `Material::resolve_pbr`
(`crates/core/src/ecs/components/material.rs`, metalness/roughness are plain `f32`),
typed particle emitter blocks, and collision-shape translation
(`crates/nif/src/import/collision/mod.rs`). Spec: `docs/engine/nifal.md`. `/audit-nifal`
owns the full canonical-translation tier (9 dimensions); this preset is the
cross-cutting regression sweep around it:
1. `/audit-nifal`
2. `/audit-nif --focus 4,5`    # parse-side geometry/import handoff (dim 4) + collision/shader blocks (dim 5)
3. `/audit-renderer --focus 6,7,17`  # NIFAL material (dim 6) + material table (dim 7) + Disney BSDF/PBR gating (dim 17)
4. `/audit-ecs`                # particle emitter components + apply_emitter_params system

## Renderer Presets

Renderer dimension map (from `/audit-renderer`, 23 dimensions): 1 AS · 2 SSBO+rays ·
3 GPU-struct layout · 4 sync/barriers · 5 memory/lifecycle · 6 NIFAL material ·
7 material table · 8 denoiser/composite · 9 skinning · 10 camera-relative precision ·
11 pipeline/render-pass · 12 cmd buffer · 13 TAA · 14 caustic splat · 15 water ·
16 volumetrics+bloom · 17 Disney BSDF/soft shadows · 18 sky/weather · 19 tangent
space · 20 debug/telemetry · 21 Cornell harness · 22 light animation ·
23 FSR3 upscaler + presentation chain (added 2026-07-27).

### `--preset renderer-deep`
After significant renderer changes — all 23 dimensions plus the cross-cutting
perf/concurrency/safety passes:
1. `/audit-renderer`
2. `/audit-performance --focus 1,2,3,5`
3. `/audit-concurrency --focus 1,2,3`
4. `/audit-safety`

### `--preset rt-deep`
After ray tracing / denoiser / G-buffer changes:
1. `/audit-renderer --focus 1,2,8`     # AS + SSBO/ray queries + denoiser/composite
2. `/audit-performance --focus 1,3`
3. `/audit-concurrency --focus 1,2`

### `--preset material-deep`
After material-table / PBR changes (`GpuMaterial` layout, dedup, SSBO,
Disney BSDF gating):
1. `/audit-renderer --focus 6,7,17`    # NIFAL material + material table + Disney BSDF
2. `/audit-safety`

### `--preset texture-roles-deep`
After changes to `MaterialTextureSet`, `ImportedMaterial`, `merge_external_material`,
or `translate_material` — the 2026-07-27 cross-game texture-role unification
(`1d94eb24` + `05d68926` + `c8c8a834`). Roles are the new per-game seam, so a
mistake here is invisible in one game and wrong in another:
1. `/audit-nifal --focus 1,8`          # material boundary (narrowed signature) + texture-role vocabulary
2. `/audit-fo4`                        # BGSM is the densest role producer
3. `/audit-starfield`                  # CDB `.mat` is the second densest, and the newest
4. `/audit-renderer --focus 6,7`       # NIFAL material + material table consumption

### `--preset upscaler-deep`
After FSR 3.1 / presentation / exposure changes. **FSR Quality is the engine
default**, so this is the default render path — a layout or barrier error here
is not a feature bug, it is every frame:
1. `/audit-renderer --focus 23,4,13`   # FSR dim + sync/barriers + TAA (shared jitter/motion vectors)
2. `/audit-safety`                     # `crates/fsr3-sys` FFI `# Safety` contracts
3. `/audit-performance --focus 1,3`
Run `BYRO_VALIDATION=1` alongside — layout errors here are structurally
invisible to `cargo test`.

### `--preset ui-deep`
After Scaleform/SWF UI changes (R4 + M48 host layer). `/audit-ui` (added
2026-08-13) now owns `crates/ui/` — the host contract, profile split, ABC
adapter, navigator and render/device lifecycle. The three generic passes stay
because the crate straddles an FFI boundary and two drift-prone generated
surfaces:
1. `/audit-ui`                         # the owner: profile, bridge, AVM2 adapter, navigator, device, input
2. `/audit-safety --focus 1`           # Ruffle/wgpu FFI + offscreen readback lifetimes
3. `/audit-concurrency --focus 7`      # Ruffle local-executor pump vs. main loop
4. `/audit-tech-debt`                  # generated AVM2 adapter + the pinned method catalogs are drift-prone

### `--preset water-deep`
After water changes. WATAL is **double-ended** and its physics half shipped at
the 2026-08-10 checkpoint, so a water change is rarely render-only — buoyancy,
submerged damping and current drag read the same canonical `WaterFlow` the
shader does:
1. `/audit-renderer --focus 1,2,8,14,15`  # AS + rays + composite + caustic splat + water dim
2. `/audit-physics --focus 6`             # the WATAL physics sink (buoyancy / damping / current)
3. `/audit-esm --focus 5`                 # the tri-state XCLW / WRLD water decode at the CELL boundary
4. `/audit-concurrency --focus 1,2`
5. `/audit-safety`

### `--preset volumetrics-deep`
After volumetric-lighting changes:
1. `/audit-renderer --focus 1,2,5,16`  # AS + rays + memory + volumetrics/bloom dim
2. `/audit-performance --focus 1,3`
3. `/audit-safety`

### `--preset bloom-deep`
After bloom-pyramid changes:
1. `/audit-renderer --focus 4,8,16`    # sync + composite + volumetrics/bloom dim
2. `/audit-performance --focus 1,3`
3. `/audit-safety`

### `--preset skin-deep`
After GPU-skinning / BLAS-refit changes (M29.x):
1. `/audit-renderer --focus 1,9`       # AS (skinned BLAS) + GPU skinning compute
2. `/audit-performance --focus 1,6`
3. `/audit-concurrency --focus 1,2,3`
4. `/audit-safety`

## Subsystem Presets

### `--preset audio-deep`
After audio (kira backend) changes — emitter/listener pose sync, spatial
sub-track lifecycle, reverb send, streaming music:
1. `/audit-audio`
2. `/audit-concurrency --focus 6,7`     # GPU/teardown ordering + worker threads
3. `/audit-safety`

### `--preset scripting-deep`
After scripting changes — the `.pex` decompiler (`crates/pex`), the `.psc`
Papyrus parser (`crates/papyrus`), the AST→ECS recognizer chain + runtime
systems (`crates/scripting`), or the cell-loader script-attach path (M30/M47):
1. `/audit-scripting`
2. `/audit-ecs`                # recognizer-emitted components + scripting-runtime systems lock/stage ordering
3. `/audit-incremental --commits 10`

### `--preset save-deep`
After save/load changes — full-ECS-snapshot capture, type-erased registry,
atomic disk write + ring, validation gates, or the M45.1 live load-apply
(`crates/save` + the engine-side driver):
1. `/audit-save`
2. `/audit-ecs`                # snapshot completeness vs component registry + frame-boundary capture safety
3. `/audit-incremental --commits 10`

### `--preset speedtree-deep`
After SpeedTree (.spt) walker / billboard-fallback changes:
1. `/audit-speedtree`
2. `/audit-incremental --commits 10`

### `--preset esm-deep`
After ESM/ESP parser changes — the GRUP walk, `SubReader` byte accounting, a
per-record schema, the FormID remap, or the CELL/WRLD walkers
(`crates/plugin/`). Add the per-game audit when one title is the target, since
`/audit-esm` audits the parser *as a parser* and the per-game skills audit its
output for their own data:
1. `/audit-esm`
2. `/audit-incremental --commits 10`
3. `/audit-<game>` — only when the change is game-specific

### `--preset physics-deep`
After physics changes — collider translation, the fixed-step accumulator, the
4-phase sync, ragdoll articulation, the character controller, or the WATAL
buoyancy sink (`crates/physics/`, `byroredux/src/ragdoll.rs`):
1. `/audit-physics`
2. `/audit-concurrency --focus 5,7`    # the resource↔storage lock dance in physics_sync_system
3. `/audit-safety`

### `--preset character-deep`
After CHARAL changes — a ruleset, a derived formula, a leveling model, or the
population boundary. `/audit-esm` joins because CHARAL's inputs (AVIF, CLAS,
NPC_) are decoded there, and a wrong FormID resolution is indistinguishable
from a wrong coefficient at the actor:
1. `/audit-character`
2. `/audit-esm --focus 4`              # AVIF / CLAS / NPC_ decode + dispatch coverage
3. `/audit-ecs`                        # ActorValues / Perks / CharacterLevel component layer

### `--preset streaming-deep`
After world-streaming / NPC-spawn changes (M40 / M41). NPC spawn is also
CHARAL's only population site and the physics registration path, so both owners
join:
1. `/audit-performance --focus 7`       # world streaming & cell transitions
2. `/audit-concurrency --focus 7`       # worker threads (streaming, debug server)
3. `/audit-character --focus 5`         # the population boundary in npc_spawn.rs
4. `/audit-physics --focus 3`           # newcomer registration / release across cell churn
5. `/audit-safety`

### `--preset legacy-deep`
After compatibility-mapping work (Gamebryo 2.3 → Redux):
1. `/audit-legacy-compat`
2. `/audit-incremental --commits 10`

## Execution

1. Parse the `--preset` argument from `$ARGUMENTS`. If unknown, list the preset
   index above and stop.
2. `mkdir -p /tmp/audit`.
3. Launch each audit in the preset as a **background agent**, max 3 concurrent.
   The audits are independent — they read the tree and write distinct reports —
   so they fan out in parallel; no ordering dependency between them.
4. Each audit writes its own report to `docs/audits/AUDIT_<TYPE>_<TODAY>.md`
   (per `_audit-common.md` finalization).
5. When all complete, produce a combined summary:

```markdown
# Audit Suite Summary — <preset> — <date>

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|-------|----------|----------|------|--------|-----|--------|
| Safety | 3 | 0 | 1 | 2 | 0 | AUDIT_SAFETY_... |
| ...   | ... | ... | ... | ... | ... | ... |

Total: X findings (C critical, H high, M medium, L low)
```

6. If any CRITICAL findings exist, warn prominently at the top of the summary.
7. For each report that has findings, suggest:
   `/audit-publish docs/audits/AUDIT_<TYPE>_<TODAY>.md`
