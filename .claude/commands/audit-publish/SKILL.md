---
description: "Convert an audit report's findings into GitHub issues with completeness checks"
argument-hint: "<path-to-audit-report>"
---

# Audit → GitHub Issues Publisher

## Process

1. **Load the report** at `$ARGUMENTS` (e.g. `docs/audits/AUDIT_RENDERER_2026-04-04.md`)

2. **Parse findings** — extract each finding block (ID, severity, location, description, status)

   **Path-validation gate (run first)**: before validating any finding, run the shared
   path gate so a report written against pre-split paths fails fast rather than producing
   issues that point at files that no longer exist:
   ```bash
   .claude/commands/_audit-validate.sh        # exit 1 on any STALE backticked path
   ```
   This is the same `#1114` / TD7-050 gate the audit skills are checked against. The
   Session 34/35 file→dir splits are exactly what it catches — a finding whose
   `Location:` points at a flat module that is now a directory (e.g. the old
   *archive.rs* → `crates/bsa/src/archive/`, *render.rs* → `byroredux/src/render/`,
   *import/walk.rs* → `crates/nif/src/import/walk/`,
   *blocks/collision.rs* → `crates/nif/src/blocks/collision/`,
   *blocks/tri_shape.rs* → `crates/nif/src/blocks/tri_shape/`) must be
   re-mapped (step 4), not filed verbatim. Note `byroredux/src/systems.rs` and
   `byroredux/src/cell_loader.rs` still exist as thin re-export shims **next to** their
   `systems/` and `cell_loader/` dirs — a `Location` there should be re-pointed to the
   owning submodule (e.g. `byroredux/src/systems/particle.rs`).

3. **Filter** — only process findings with status **NEW**. Skip Existing/Regression (already tracked).

4. **Validate each finding** against current code:
   - Read the referenced file at the specified lines
   - **Re-map before judging**: if the `Location:` file no longer exists but the code does
     (a module split, not a fix), resolve it to its current submodule and update the
     finding's path/line — do NOT mark it STALE/UNVERIFIABLE for a path move alone. Line
     numbers drift after a split; trust the symbol (`grep -rn <fn/struct>`), not the line.
     Examples seen in the wild: material logic now lives at
     `byroredux/src/material_translate.rs` (`translate_material`) +
     `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`), not in the old
     import/material call sites; the per-frame particle system is
     `byroredux/src/systems/particle.rs` (`apply_emitter_params`), not a flat `systems.rs`.
   - Mark as: **CONFIRMED** (issue exists), **STALE** (already fixed), **UNVERIFIABLE** (can't confirm)
   - Skip STALE findings

5. **Deduplication check**:
   ```bash
   gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state
   ```
   Skip findings that match an existing OPEN issue title/description.

6. **Completeness checks** — for each CONFIRMED finding, generate checkboxes:

   ```markdown
   ## Completeness Checks
   - [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
   - [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
   - [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
   - [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
   - [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
   - [ ] **CANONICAL-BOUNDARY**: If fix touches `byroredux/src/material_translate.rs::translate_material`, `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the import-walk emitter params (`crates/nif/src/import/walk/mod.rs::extract_emitter_params`/`extract_emitter_rate`), verify per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into the shaders/renderer and never re-derived at render time. See also `/audit-nifal`.
   - [ ] **TESTS**: Regression test added for this specific fix
   ```

7. **Create GitHub issues** for each CONFIRMED + NEW finding:
   ```bash
   gh issue create --repo matiaszanolli/ByroRedux \
     --title "<ID>: <title>" \
     --body "<finding details + completeness checks>" \
     --label "<severity>,<domain>,bug"
   ```

   **Domain vocabulary**: the canonical `<domain>` list lives at `.claude/commands/_audit-common.md`
   (the "Domain Labels" section). It currently enumerates
   `ecs, renderer, vulkan, pipeline, memory, sync, platform, cxx, nif, bsa, esm, animation, legacy-compat, performance, safety, tech-debt`.
   With 19 crates now under `crates/` (including `crates/audio/`, `crates/spt/`,
   `crates/sfmaterial/`, `crates/debug-ui/`), a finding in one of those subsystems has no
   matching label yet. Use the closest existing domain (e.g. `renderer` for `debug-ui`'s
   egui pass, `nif`/`bsa` for `sfmaterial` CDB parsing) and flag the gap; if the new
   `audio`/`spt`/`sfmaterial`/`debug-ui` labels get added to the canonical list in
   `_audit-common.md`, prefer them. (NIFAL canonical-translation findings map to the
   subsystem they live in — `nif`/`renderer` — and should cross-link `/audit-nifal`.)

   **Audit-type label table**: the report family (parsed from the `AUDIT_<TYPE>_<DATE>.md`
   filename) selects the type label and any extra family label. Default is `bug`; the rest:

   | Report (`AUDIT_<TYPE>_*.md`) | Default `<domain>` | Type label | Extra label |
   |------------------------------|--------------------|------------|-------------|
   | `AUDIT_TECH_DEBT_*`          | (per finding)      | `maintenance` | `tech-debt` |
   | `AUDIT_RENDERER_*`           | `renderer`         | `bug`      | —           |
   | `AUDIT_STARFIELD_*` / `AUDIT_FNV_*` / `AUDIT_FO3_*` / `AUDIT_FO4_*` | `nif` (+ per finding) | `bug` | `legacy-compat` |
   | `AUDIT_AUDIO_*`              | `audio` (fallback `renderer`) | `bug` | — |
   | `AUDIT_CONCURRENCY_*`        | `sync`             | `bug`      | —           |
   | `AUDIT_ECS_*`                | `ecs`              | `bug`      | —           |
   | `AUDIT_LEGACY_COMPAT_*`      | `legacy-compat`    | `bug`      | —           |
   | `AUDIT_INCREMENTAL_*`        | (per finding)      | `bug`      | —           |
   | `AUDIT_NIFAL_*`              | `nif`              | `bug`      | —           |

   Per-finding domain always wins over the table default. For `AUDIT_TECH_DEBT_*.md` the
   final set is `<severity>,<domain>,tech-debt,maintenance` (tech debt isn't a bug). Any
   future family follows the same shape (e.g. a hypothetical `AUDIT_DOCS_*.md` → `docs`
   label + `documentation` type).

8. **Save to local tracking**:
   ```bash
   mkdir -p .claude/issues/<NUMBER>
   ```
   Write `ISSUE.md` with the finding details.

   **Immutable-snapshot convention** (TD10-001 / #1156): `.claude/issues/<N>/ISSUE.md` is a snapshot of the issue **as filed**, not a live mirror. GitHub is the authoritative source for current state — query via `gh issue view <N> --json state` when you need the live state. Do NOT write a `State:` or `Status:` field; if writing the body via `gh issue view --json body`, the state in the resulting JSON reflects fetch time, not now. Audits that need live state should query GitHub, not read the local file. This convention applies symmetrically to `INVESTIGATION.md` and any sibling files created by `/fix-issue`.

9. **Summary** — print a table:
   | Finding | Action | Reason |
   |---------|--------|--------|
   | REN-001 | Created #42 | NEW, CONFIRMED |
   | REN-002 | Skipped | Existing #38 |
   | REN-003 | Skipped | STALE |
   | NIF-004 | Created #43 | NEW, CONFIRMED |

10. **Suggest next step**: For each created issue, note:
    ```
    Fix with: /fix-issue <number>
    ```
