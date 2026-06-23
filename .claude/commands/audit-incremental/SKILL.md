---
description: "Delta audit — check only recently changed code for regressions and new bugs"
argument-hint: "[--working] [--commits <N>] [--range <A>..<B>] [--since <date>]"
---

# Incremental / Delta Audit

Audit **only what changed**, not a whole subsystem. The goal is to catch
*new* bugs and *regressions* introduced by recent work — fast — by routing
each changed file to the subsystem/per-game audit dimensions that own it
and applying their checks to the diff alone.

This is a meta-audit: it does not define dimensions, it *dispatches* to the
real audit skills (each lives at `.claude/commands/audit-<NAME>/SKILL.md`).
Use those for the authoritative checklist of any one area.

See `.claude/commands/_audit-common.md` for project layout, severity,
methodology, deduplication, context rules, and the base finding format.
See `.claude/commands/_audit-severity.md` for the severity scale + special
rules (the NIFAL / GPU-struct / AS / SSBO rows are the ones a delta most
often trips).

## Step 1: Determine the diff scope

Pick the narrowest scope that covers the work under review. Default to the
working tree if nothing is specified.

| Argument | Scope | Diff command |
|----------|-------|--------------|
| *(none)* / `--working` | uncommitted work | `git diff HEAD --name-only` (add `--staged`-less; for staged-only use `git diff --staged --name-only`) |
| `--commits <N>` | last N commits | `git diff "HEAD~<N>..HEAD" --name-only` |
| `--range <A>..<B>` | explicit revision range | `git diff "<A>..<B>" --name-only` |
| `--since <date>` | everything since a date | base=`$(git log --since="<date>" --format="%H" \| tail -1)`; then `git diff "${base}^..HEAD" --name-only` |

Then pull the actual hunks for the same scope (swap `--name-only` for
nothing, or `-U6` for more context) and the commit log:

```bash
git diff HEAD~10..HEAD --stat        # changed-file overview (substitute your scope)
git diff HEAD~10..HEAD               # the hunks you will actually audit
git log --oneline HEAD~10..HEAD      # commit themes, PR numbers, milestone tags
```

Audit the **diff**, with just enough surrounding context to confirm each
finding (`_audit-common.md` § Methodology) — do not re-audit untouched code.

## Step 2: Route each changed file to its audit dimension

Map every changed path to the audit skill(s) that own it, then apply that
skill's checks to the diff. A file can hit multiple rows (e.g. a shader +
its `#[repr(C)]` host struct → renderer **and** the GPU-struct-sync rule).
Risk is the *floor* severity for an un-disproven finding in that area.

| Changed path | Owning audit(s) | Risk |
|--------------|-----------------|------|
| `crates/renderer/src/vulkan/**` (pipeline, sync, descriptors, context/) | `/audit-renderer`, `/audit-safety`, `/audit-concurrency` | HIGH |
| `crates/renderer/src/vulkan/acceleration/**`, `svgf.rs`, `gbuffer.rs`, `composite.rs` (RT / denoise / G-buffer) | `/audit-renderer` | HIGH |
| `crates/renderer/src/vulkan/scene_buffer/**`, `material.rs` (`#[repr(C)]` GPU structs) | `/audit-renderer`, `/audit-nifal` | HIGH |
| `crates/renderer/src/vulkan/volumetrics.rs` + `shaders/volumetrics_*.comp` (M55) | `/audit-renderer` | HIGH |
| `crates/renderer/src/vulkan/bloom.rs` + `shaders/bloom_*.comp` (M58) | `/audit-renderer` | HIGH |
| `crates/renderer/src/vulkan/water.rs`, `shaders/water.vert`/`water.frag`, `byroredux/src/systems/water.rs`, `byroredux/src/cell_loader/water.rs` (M38) | `/audit-renderer`, `/audit-fnv` | HIGH |
| `crates/renderer/shaders/**` (any `.comp`/`.vert`/`.frag`) | `/audit-renderer` (+ GPU-struct-sync rule) | HIGH |
| `crates/core/src/ecs/**` | `/audit-ecs`, `/audit-concurrency` | HIGH |
| `crates/nif/src/blocks/**`, `crates/nif/src/import/**`, `crates/nif/src/anim/**` | `/audit-nif`; per-game `/audit-<game>` | HIGH |
| `crates/bsa/src/**` (BSA / BA2 / CSG) | `/audit-nif` (archive feed), per-game `/audit-<game>` | HIGH |
| `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`, `crates/nif/src/import/collision.rs` (NIFAL boundary) | `/audit-nifal` | HIGH |
| `byroredux/src/env_translate.rs` (EXAL boundary) | `/audit-nifal` (mirror), `/audit-renderer` | MEDIUM |
| `byroredux/src/ragdoll.rs`, `crates/physics/src/**` (PHYSAL / Rapier bridge) | `/audit-safety`, per-game `/audit-<game>` | MEDIUM |
| `crates/spt/src/**`, `byroredux/src/cell_loader/refr.rs` (.spt route) | `/audit-speedtree` | MEDIUM |
| `crates/plugin/src/esm/**` (incl. `records/misc/{water,character,world,ai,magic,effects,equipment}.rs`) | per-game `/audit-<game>`, `/audit-legacy-compat` | MEDIUM |
| `crates/core/src/animation/**` | `/audit-nif` (anim import), `/audit-ecs` | MEDIUM |
| `byroredux/src/cell_loader/**` | per-game `/audit-<game>` | MEDIUM |
| `byroredux/src/systems/**`, `byroredux/src/render/**` | `/audit-ecs`, `/audit-renderer`, `/audit-performance` | MEDIUM |
| `byroredux/src/scene/**` | per-game `/audit-<game>` | MEDIUM |
| `byroredux/src/main.rs`, `byroredux/src/commands/**` | `/audit-ecs` | MEDIUM |
| `crates/scripting/**`, `crates/pex/**`, `crates/papyrus/**` | `/audit-scripting` | MEDIUM |
| `crates/save/**` | `/audit-save` | MEDIUM |
| `byroredux/src/asset_provider.rs` (sibling-BSA auto-load, AE path strip) | per-game `/audit-<game>` | MEDIUM |
| `crates/audio/src/{lib,tests}.rs` | `/audit-audio` | MEDIUM |
| `crates/sfmaterial/src/**` (Starfield CDB) | `/audit-starfield` | MEDIUM |
| `crates/bgsm/src/**` (FO4+ BGSM/BGEM) | `/audit-fo4`, `/audit-nifal` | MEDIUM |
| `crates/facegen/src/**` | per-game `/audit-<game>` | MEDIUM |
| `crates/debug-ui/src/**`, `crates/renderer/src/vulkan/egui_pass.rs` (egui overlay → touches `draw_frame`) | `/audit-renderer`, `/audit-concurrency` | MEDIUM |
| `crates/papyrus/src/**`, `crates/scripting/src/**` | `/audit-tech-debt` (no dedicated skill) | MEDIUM |
| `**/tests/**`, `**/*_tests.rs`, `byroredux/tests/golden_frames.rs` | `/audit-regression` | LOW |
| `*.md`, `docs/**` | `/audit-tech-debt` (doc rot) | LOW |

> Layout shifts that often surprise a delta audit: `render.rs`,
> `systems.rs`, `scene.rs`, and `cell_loader.rs` are all directories
> now (`byroredux/src/render/`, `systems/`, `scene/`, `cell_loader/`),
> each with a thin `mod.rs` dispatch + topic submodules + `*_tests.rs`
> siblings. `crates/renderer/src/vulkan/acceleration/` and `scene_buffer/`
> are likewise split. The authoritative tree is in `_audit-common.md`
> § Project Layout — route against it, not against memory.

## Step 3: Regression-focused checks on each changed file

For every changed file, read the hunk + minimal context and ask:

- [ ] **New bug** — logic error, off-by-one, wrong byte width, missing
      version/era gate (B-splines reach FNV/FO3, not just Skyrim+).
- [ ] **Contract break** — did a public signature change without *all*
      call sites updating? (`git grep` the symbol across the workspace.)
- [ ] **Silent divergence** — was a value built at two sites and only one
      edited? The classic NIFAL leak: the two `Material` load paths.
- [ ] **Unsafe delta** — new `unsafe` block, changed safety invariant, or
      a safety comment that no longer matches the body (`_audit-severity`:
      MEDIUM floor for unsafe-without-comment).
- [ ] **Lock / query delta** — changed RwLock scope or a new multi-component
      query? Verify TypeId-sorted acquisition (deadlock → HIGH floor).
- [ ] **Vulkan delta** — new pipeline/barrier/sync, AS build/refit, or
      descriptor write? Missing barrier or wrong AS geometry → see the
      severity special-rules table (HIGH/CRITICAL floors).
- [ ] **GPU-struct lockstep** — a touched `#[repr(C)]` struct
      (`GpuInstance`/`GpuCamera`/`GpuMaterial`/`GpuLight`) **and** its
      mirror in every shader that reads it. Size/offset drift → HIGH.
- [ ] **Missing test** — a changed code path with no corresponding test
      update. Flag in the "Missing Tests" section even if the code is right.

### Rust-specific deltas

- [ ] **Drop ordering** — Vulkan destruction order still reverse of build?
- [ ] **Error handling** — new `unwrap()`/`expect()` on a recoverable path?
- [ ] **Lifetimes** — a borrow whose scope changed (temporary outliving / new dangling borrow)?
- [ ] **Trait impls** — a new impl consistent with the existing family (Component storage decl, Send+Sync)?

### NIFAL boundary delta (when the diff touches the material rows)

The canonical material contract is **resolve-once at the translation
boundary, no render-time fallback** — so a wrong value there is silently
wrong across every game.

- `Material::metalness` / `roughness` are plain resolved `f32`
  (`crates/core/src/ecs/components/material.rs`), not `Option`. They are
  finalized by `Material::resolve_pbr`, which clamps and — only when the
  upstream override arrived `NaN` — falls back to `classify_pbr_keyword`
  (the surviving keyword classifier; it is a sentinel-backstop for
  non-pre-classified sources, **not** a per-draw safety net).
- NIF-imported content is pre-classified at import (`classify_legacy_pbr`)
  so `resolve_pbr` only clamps; BGSM/BGEM also arrive pre-classified.
  Confirm a diff did not leave either scalar `NaN` at draw time.
- Both load paths must still route through `translate_material`:
  `byroredux/src/cell_loader/spawn.rs` (REFR spawn) and
  `byroredux/src/scene/nif_loader.rs` (loose-NIF). If a diff adds a field
  to one and not the other, that is the divergence this layer exists to
  prevent. Defer to `/audit-nifal` for the full single-boundary checklist.

### NIFAL particle / collision chain (multi-file translation surfaces)

These are not single-file changes — a diff to one tier is incomplete
without the others:

- **Particle emitter:** typed blocks
  (`crates/nif/src/blocks/particle.rs`: `NiPSysEmitter` /
  `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier`)
  → extraction (`crates/nif/src/import/walk/mod.rs`:
  `extract_emitter_params` / `extract_emitter_rate`) → system
  (`byroredux/src/systems/particle.rs`: `apply_emitter_params`). Audit all three.
- **Collision shape:** a new `Bhk*Shape` parser
  (`crates/nif/src/blocks/collision/`) is also a translation surface —
  `crates/nif/src/import/collision.rs` must map it to `CollisionShape`,
  or it is silently dropped (MEDIUM floor; HIGH if it removes visible
  game content). PHYSAL now consumes ragdoll constraints, so a collision
  diff can ripple into `byroredux/src/ragdoll.rs` + `crates/physics/`.

## Step 4: Deduplicate

Run the dedup pass from `_audit-common.md` § Deduplication for every
finding (existing-issue search + prior-report scan) before recording it.
A regression of a *closed* issue is reported as "Regression of #NNN".

## Extra Per-Finding Field

In addition to the base format in `_audit-common.md`:

- **Changed in**: `<file-path>` (commit `<hash>` / working tree)

## Output

Write to: **`docs/audits/AUDIT_INCREMENTAL_<TODAY>.md`** (YYYY-MM-DD).

### Report structure
1. **Change summary** — scope (range/commits/since), files changed, themes.
2. **Routing map** — each changed file → dimension(s) it was audited under.
3. **Findings** — new bugs + regressions (base format + `Changed in`).
4. **Missing tests** — changed code paths with no test update.

Then suggest:

```
/audit-publish docs/audits/AUDIT_INCREMENTAL_<TODAY>.md
```
