---
name: "source-command-audit-common"
description: "Migrated source command `_audit-common`"
---

# source-command-audit-common

Use this skill when the user asks to run the migrated source command `_audit-common`.

## Command Template

# Shared Audit Protocol — ByroRedux

Referenced by every audit skill. Not a slash command (leading `_`). Keep this file lean: it is loaded on every audit run.

## Layout, ownership, references

- **Tree**: `AGENTS.md` § Workspace Structure is authoritative and already in context. Never trust a hand-typed tree in a skill — `ls` / `git ls-files` it.
- **Who owns what**: `.Codex/commands/_audit-owners.md` (path → owner audit → risk floor). One table; `/audit-incremental` routes from it and `_audit-validate.sh` flags unowned code. Do not restate ownership inside a skill.
- **Layout traps** (each has made an audit stale): thin-dispatch **file + directory pairs** exist — check both: `byroredux/src/{scene,cell_loader,systems,save_io,npc_spawn}.rs` beside `{…}/`, `crates/renderer/src/vulkan/{volumetrics,sky_cube}.rs` beside `{…}/` (the dir holds only sub-parts; the `.rs` stays the bulk). `acceleration/`, `scene_buffer/`, `context/`, `boot/`, `render/` are genuine splits behind a thin `mod.rs`. `crates/nif/src/blocks/shader/` is a directory. `ImportedMaterial` is nested at `ImportedMesh.material`, not flat fields. `crates/cxx-bridge` (36 LOC) and `crates/platform` (60 LOC) are placeholders — the live FFI is `crates/fsr3-sys` + Ruffle/wgpu.
- **Reference docs** (code-verified; cite, don't paraphrase). Renderer: `docs/engine/{renderer,shader-pipeline,memory-budget,shadow-pipeline-tradeoffs,rt-lighting-material-recovery,physical-lighting-backbone,fsr3-upscaler-integration-plan}.md`. Abstraction layers: `nifal.md`, `exal.md` (+`exal-groundcover.md`, `exal-trees.md`), `skyal.md`, `watal.md`, `physal.md`, `charal.md` (+six `charal-*-ruleset.md`), `packal.md`. Data: `plugin-loading.md`, `esm-records.md`, `nif-parser.md`, `archives.md`, `fo4-csg-format.md`. Runtime traces: `pipeline-overview.md`, `exterior-grid-streaming.md`, `save-load-roundtrip.md`, `npc-spawn-ai-packages.md`, `playable-vertical-slice.md`. Other: `ecs.md`, `animation.md`, `physics.md`, `scripting.md`, `papyrus-parser.md`, `ui.md`, `launcher.md`, `debug-cli.md`, `testing.md`; status floor `docs/feature-matrix.md` (re-check rows against code). Per-game survey: `per-game-translation-survey.md`, `game-compatibility.md`.
- **Shared vocabulary pin** (restored 2026-09-20 after `a43b19603`'s slim-down dropped it; the count is load-bearing — `material_translate`'s `documented_texture_role_list_matches_the_struct` audits this file, #3904/#3465): `MaterialTextureSet<T>` (`crates/nif/src/import/types.rs`) replaces per-game texture slot numbers with 22 named roles (source-agnostic) + `decals: [T; 4]`. NiTexturingProperty / BSShaderTextureSet / BGSM / BGEM / BSEffectShaderProperty all populate the SAME roles — game-specific slot numbers must not survive past the NIF import boundary.

## Game Data Locations

```
Oblivion:   /mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/
Fallout 3:  /mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/
Fallout NV: /mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/
Skyrim LE:  /home/matias/Games/skyrim-original/drive_c/Program Files (x86)/The Elder Scrolls V Skyrim/Data/
            (2011 release, Wine prefix; BSA v104 zlib, HEDR 0.94 / form v40 — not SE's v105 LZ4 / 1.71 / v44;
            tests read it via BYROREDUX_SKYRIMLE_DATA, falling back to this prefix — crates/nif/tests/common/mod.rs)
Skyrim SE:  /mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/
Fallout 4:  /mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/
Fallout 76: /mnt/data/SteamLibrary/steamapps/common/Fallout76/Data/
Starfield:  /mnt/data/SteamLibrary/steamapps/common/Starfield/Data/
Gamebryo 2.3 source: /media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/
  (CoreLibs/NiMain scene graph · NiAnimation · NiCollision · NiSystem · SDK/Win32/Include 1,592 headers)
```

Severity scale: `.Codex/commands/_audit-severity.md`.

## Methodology

- Be skeptical; assume bugs exist even where code "looks fine". Re-read the code path for every claim. After a finding, try to disprove it; keep only what survives.
- Prefer concrete evidence (call sites, data flow, configs) over assumption. Grep before Read; paginate (≤1500 lines/Read); write findings incrementally; finish one dimension before the next.
- Rust: read the SAFETY comment around every `unsafe`; trace lifetimes through borrows; check `Send + Sync` on Component/Resource types; verify destroy-before-parent for Vulkan objects; check TypeId-sorted lock acquisition for multi-component queries; cite the Khronos spec for Vulkan behaviour.
- Hot-path hashing (#2923): the per-frame render/skinning path is `FxHashMap`/`FxHashSet` end-to-end (`SkinSlotPool`, its `pose_dirty` set, `FrameInputs.pose_dirty`, `skin_offsets` in `byroredux/src/render/`); a reintroduced std `HashMap`/`HashSet` there is the regression (guard: the `must stay FxHashSet (#2923)` assertion in `crates/renderer/src/vulkan/context/mod.rs`). Hot-path rule only — std hashing in load-time/parser/DoS-facing code is fine.

## Delta-first scoping (default for every audit)

1. Find the last report for this audit type: `ls docs/audits/AUDIT_<TYPE>_*.md | sort | tail -1` → date **D** (and its recorded HEAD).
2. Each dimension carries `Paths:` and `First step:` lines. Run `git log --since=D --format='%h %s' -- <Paths>` per dimension. A dimension with **no commits** since D gets one line ("unchanged since D — guard spot-checked") plus its `First step:`; spend the budget on changed dimensions.
3. `--full` (or an explicit `--focus <dimensions>`) overrides. A first-ever run is always full.
4. Re-verify a prior finding only via `/audit-regression`; here, dedup against it (below).

## Path-Reference Convention

Backticked file/dir paths and snake_case symbols in any skill (including this file) assert "exists right now". `.Codex/commands/_audit-validate.sh` fails on stale paths and lists missing symbols as advisory. Historical, deleted or planned things go in *italics* or plain text; memory-slug names are italic, never backticked. Run the script before committing any skill edit; clear the advisory list rather than scrolling past it (it is what caught `GpuMaterial` documented at the wrong size).

**Never write an instruction to not look.** A skill may record a dated "known-open — do NOT re-litigate" fact pointing at the owning doc/issue (a rotting fact is checkable and correctable). It must never tell an auditor to "confirm absence rather than report it" — that becomes a blindfold over the newest, least-reviewed code and no gate can catch it (#3199).

## Deduplication (MANDATORY)

1. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
2. Search the finding's keywords in issue titles and in `docs/audits/`.
3. OPEN → "Existing: #NNN", skip. CLOSED → verify the fix is in place; if regressed, "Regression of #NNN". No match → NEW.
4. **Reports dated before 2026-06-07 predate `/audit-publish`: an absent issue is NOT evidence the finding is open** (26 of 131 pre-cutoff CRITICAL/HIGH findings have no issue; a 3-of-3 spot check found all silently fixed). Verify such findings against the code and say which you checked.

## Base Per-Finding Format

```
### <ID>: <Short Title>
- **Severity**: CRITICAL | HIGH | MEDIUM | LOW
- **Dimension**: <audit area>
- **Location**: `<file-path>:<line-range>`
- **Status**: NEW | Existing: #NNN | Regression of #NNN
- **Description**: what is wrong and why
- **Evidence**: code snippet or exact call path
- **Impact**: what breaks, when, blast radius
- **Related**: findings / issues
- **Suggested Fix**: 1-3 sentences
```

Deep audits add fields (Trigger Conditions, Flow, Changed File …) — see each skill.

## Issue Labels

Only labels that exist (`gh label list --repo matiaszanolli/ByroRedux`); `/audit-publish` never creates labels. One **severity**, one **type**, ≥1 **domain**, **game** only when title-specific.
- Severity: `critical` `high` `medium` `low` `info`. Type: `bug` `enhancement` `documentation`.
- Domain: `ecs` `renderer` `vulkan` `pipeline` `shaders` `memory` `sync` `concurrency` `cxx` `nif` `nif-parser` `nifal` `import-pipeline` `esm-plugin` `animation` `physics` `character` `water` `terrain-exterior` `speedtree` `audio` `ui` `save-load` `scripting` `gameplay` `ai` `combat` `dialogue` `inventory` `quests` `legacy-compat` `performance` `safety` `tech-debt` `doc-rot` `test-gap` `info`.
- Game: `game:fnv` `game:fo3` `game:fo4` `game:fo76` `game:skyrim` `game:oblivion` `game:starfield`.
- `sync` = GPU (semaphores/fences/barriers/queues); `concurrency` = CPU (ECS locks, scheduler access, `RwLock`, races).
- No label of their own — map to the closest and flag it in the publish summary: BSA/BA2/CSG readers, FaceGen → `import-pipeline`; platform, debug-server/`byro-dbg`, SDK, launcher, audit infrastructure (`.Codex/commands/`) → `tech-debt`. There is no `bsa` / `platform` / `debug-ui` / `maintenance` label.

## Report Finalization

1. Save to `docs/audits/AUDIT_<TYPE>_<YYYY-MM-DD>.md`. Open with: `**HEAD**: <short sha> · **Baseline**: <previous report or "none"> · **Audited**: <dims> · **Unchanged since baseline (skimmed)**: <dims>` so the next run can delta-scope.
2. Do NOT create GitHub issues. Tell the user: `/audit-publish docs/audits/AUDIT_<TYPE>_<date>.md`.
