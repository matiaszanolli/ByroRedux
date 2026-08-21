---
description: "Convert an audit report's findings into GitHub issues with completeness checks"
argument-hint: "<path-to-audit-report>"
---

# Audit → GitHub Issues Publisher

Turn a finished audit report (`docs/audits/AUDIT_<TYPE>_<DATE>.md`) into one GitHub
issue per actionable finding, with dedup, label reconciliation, and a completeness
gate so nothing silently slips through.

Shared protocol (read, do not restate): `.claude/commands/_audit-common.md` —
the **Base Per-Finding Format** (the exact field set this skill parses) and the
**Deduplication (MANDATORY)** flow. Severity scale: `.claude/commands/_audit-severity.md`.

This skill is the *only* place issues are created. Audit skills stop at writing the
report; they never call `gh issue create`.

## Process

### 1. Load + parse the report

Read `$ARGUMENTS` (e.g. `docs/audits/AUDIT_RENDERER_2026-04-04.md`). Each finding block
follows _audit-common's Base Per-Finding Format: `### <ID>: <Title>` then `Severity`,
`Dimension`, `Location`, `Status`, `Description`, `Evidence`, `Impact`, `Related`,
`Suggested Fix`. Extract those fields per finding; ID + Severity + Location + Status
are required, the rest carry into the issue body.

### 2. Path-validation gate (run first, before judging any finding)

```bash
.claude/commands/_audit-validate.sh        # exit 1 on any STALE backticked path
```

This is the `#1114` / TD7-050 gate. It fails fast when a report was written against
pre-split paths — a `Location:` pointing at a file that is now a directory. Common
splits to expect (old single file → current dir; the old paths are
deliberately un-backticked since they no longer exist): *archive.rs* →
`crates/bsa/src/archive/`, *render.rs* → `byroredux/src/render/`,
*import/walk.rs* → `crates/nif/src/import/walk/`, *blocks/collision.rs* →
`crates/nif/src/blocks/collision/`, *blocks/tri_shape.rs* →
`crates/nif/src/blocks/tri_shape/`.
`byroredux/src/systems.rs` and `byroredux/src/cell_loader.rs` survive as thin re-export
shims **beside** their `systems/` and `cell_loader/` dirs — re-point a `Location` there
to the owning submodule (e.g. `byroredux/src/systems/particle.rs`).

### 3. Filter by status

Process only findings with status **NEW**. `Existing: #NNN` and `Regression of #NNN`
are already tracked upstream — record them in the summary, do not re-file.

### 4. Validate each NEW finding against current code

- Read the referenced file at the symbol (not the line — line numbers drift; trust
  `grep -rn <fn/struct>`).
- **Re-map before judging.** If the `Location:` file no longer exists but the code does
  (a split, not a fix), resolve to the current submodule and update the finding's path
  before filing. Do NOT mark a path move as STALE. Examples: material logic lives at
  `byroredux/src/material_translate.rs` (`translate_material`) +
  `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`); the per-frame
  particle system is `byroredux/src/systems/particle.rs` (`apply_emitter_params`), fed by
  `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`).
- Classify: **CONFIRMED** (bug still present) → file it; **STALE** (already fixed) → skip,
  record in summary; **UNVERIFIABLE** (cannot confirm against code) → skip, record in summary.

### 5. Deduplicate against open issues

Follow _audit-common's **Deduplication** flow:

```bash
mkdir -p /tmp/audit
gh issue list --repo matiaszanolli/ByroRedux --limit 400 --json number,title,state,labels \
  > /tmp/audit/issues.json
```

Match each CONFIRMED finding's keywords against existing **open** issue titles/bodies.
On a match, skip and record `Existing #NNN` in the summary. If a *closed* issue matches
and the bug is back, file it but title/note it as a regression of `#NNN`.

### 6. Reconcile labels against the live repo (do this once, before any create)

The set of labels that exist in the repo is authoritative — `gh issue create` rejects an
unknown label. Pull the live set first:

```bash
gh label list --repo matiaszanolli/ByroRedux --limit 200 --json name --jq '.[].name' \
  > /tmp/audit/labels.txt
```

Every label this skill applies MUST appear in that file. The four axes are defined in
_audit-common's **Issue Labels** section — do not restate them; the summary below is the
publish-time mapping only.

**Severity** (always exactly one): `critical` · `high` · `medium` · `low` · `info`.

**Type** (always exactly one): `bug` · `enhancement` · `documentation`. There is **no**
`maintenance` label — tech-debt findings use the `tech-debt` domain label plus `bug`;
doc-rot findings use `documentation` + `doc-rot`.

**Domain** (one or more) — as of 2026-08-21 nearly every audited subsystem has its own
label. Map the finding's subsystem directly:

| Finding subsystem | Domain label(s) |
|-------------------|-----------------|
| NIF parser / block dispatch | `nif-parser` (primary) + `nif` (format tag) |
| NIFAL canonical translation | `nifal` + the subsystem it lands in (`nif-parser` / `renderer`) |
| ESM / CELL / WRLD / plugin loading | `esm-plugin` |
| BSA / BA2 / CSG archive readers | `import-pipeline` *(no `bsa` label — flag the gap)* |
| Vulkan renderer / RT / denoiser | `renderer` (+ `vulkan` / `pipeline` / `memory`) |
| GLSL / SPIR-V sources, shader contract | `shaders` (+ `renderer`) |
| Water — WATAL, buoyancy, waterline | `water` |
| Terrain / LOD / sky / weather / worldspace (EXAL) | `terrain-exterior` |
| Physics — Havok→Rapier, colliders, ragdoll (PHYSAL) | `physics` |
| SpeedTree (`.spt`) | `speedtree` |
| Character rulesets — ActorValues, perks, leveling (CHARAL) | `character` |
| Audio (M44) | `audio` |
| Scaleform / SWF UI (R4 + M48) | `ui` |
| Save/load (M45) | `save-load` |
| Papyrus / ObScript / scripting runtime | `scripting` |
| ECS storage / queries / scheduler | `ecs` |
| GPU sync (semaphores, fences, barriers) | `sync` |
| CPU lock ordering / races / access declarations | `concurrency` |
| Platform / windowing, debug-server, audit infrastructure | `tech-debt` *(no own label — flag the gap)* |

**Game** (zero or more) — apply whenever the finding is specific to a title, *in addition*
to the domain label. A per-game audit report labels every finding with its game; a
cross-cutting audit labels only the findings that name a specific title's data:
`game:fnv` · `game:fo3` · `game:fo4` · `game:fo76` · `game:skyrim` · `game:oblivion` ·
`game:starfield`.

**Cross-cutting kind tags** (zero or more, alongside the domain):
- `doc-rot` — the defect is documentation drifted from code (stale ROADMAP row, SKILL doc
  naming a deleted symbol, a comment describing removed behaviour). Pair with
  `documentation` as the type.
- `test-gap` — the defect is missing/vacuous/non-asserting coverage. Pair with the domain
  of the code left untested.
- `tech-debt` — dead code, duplication, stale markers, magic numbers, oversized files.

If a finding genuinely has no reasonable existing label, file it with severity + `bug`
only and flag the missing-label gap in the summary. Do **not** `gh label create` ad hoc —
new labels are a deliberate repo decision, not a per-publish side effect.

**Report-family defaults** — the `AUDIT_<TYPE>_<DATE>.md` filename selects a default
domain/type; a per-finding `Dimension`/domain always overrides it:

| Report (`AUDIT_<TYPE>_*.md`) | Default domain | Type | Extra |
|------------------------------|----------------|------|-------|
| `AUDIT_RENDERER_*` | `renderer` | `bug` | — |
| `AUDIT_ECS_*` | `ecs` | `bug` | — |
| `AUDIT_CONCURRENCY_*` | `concurrency` | `bug` | `sync` on GPU-side findings |
| `AUDIT_NIF_*` | `nif-parser` | `bug` | `nif` |
| `AUDIT_NIFAL_*` | `nifal` | `bug` | + the landing subsystem |
| `AUDIT_ESM_*` | `esm-plugin` | `bug` | — |
| `AUDIT_PHYSICS_*` | `physics` | `bug` | `water` on the WATAL buoyancy sink |
| `AUDIT_CHARACTER_*` | `character` | `bug` | — |
| `AUDIT_UI_*` | `ui` | `bug` | — |
| `AUDIT_SAVE_*` | `save-load` | `bug` | — |
| `AUDIT_AUDIO_*` | `audio` | `bug` | — |
| `AUDIT_SPEEDTREE_*` | `speedtree` | `bug` | `terrain-exterior` |
| `AUDIT_SCRIPTING_*` | `scripting` | `bug` | — |
| `AUDIT_FNV_*` / `AUDIT_FO3_*` / `AUDIT_FO4_*` / `AUDIT_SKYRIM_*` / `AUDIT_OBLIVION_*` / `AUDIT_STARFIELD_*` | (per finding) | `bug` | the matching `game:*` on **every** finding + `legacy-compat` |
| `AUDIT_LEGACY_COMPAT_*` | `legacy-compat` | `bug` | `game:*` where title-specific |
| `AUDIT_TECH_DEBT_*` | (per finding) | `bug` | `tech-debt` |
| `AUDIT_INCREMENTAL_*` / `AUDIT_REGRESSION_*` | (per finding) | `bug` | — |

For `AUDIT_TECH_DEBT_*` the final set is `<severity>,<domain?>,tech-debt,bug`. Doc-rot
findings — in any report family — swap `bug` → `documentation` and add `doc-rot`;
coverage findings add `test-gap`. For everything else: `<severity>,<domain>,bug`,
plus `game:<title>` whenever the finding is specific to one game's data.

### 7. Build the completeness checklist (per CONFIRMED finding)

Append to each issue body. Drop rows that can't apply (e.g. omit FFI if the fix is
NIF-only):

```markdown
## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **FFI**: If the cxx bridge is touched, pointer lifetimes across the boundary are sound
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
```

### 8. Create the issue

```bash
gh issue create --repo matiaszanolli/ByroRedux \
  --title "<ID>: <title>" \
  --body  "<description + evidence + impact + suggested fix + completeness checks>" \
  --label "<severity>,<domain>,<type>"
```

`--label` takes a comma-separated list. Every token must be in `/tmp/audit/labels.txt`
(step 6). If `gh` rejects a label, the reconciliation missed one — fix the mapping, do not
drop the finding silently.

### 9. Snapshot to local tracking

```bash
mkdir -p .claude/issues/<NUMBER>
```

Write `.claude/issues/<NUMBER>/ISSUE.md` with the finding details.

**Immutable-snapshot convention** (TD10-001 / #1156): this file is the issue *as filed*,
not a live mirror. GitHub is authoritative for current state — query
`gh issue view <N> --json state` when live state is needed. Do **not** write a
`State:`/`Status:` field. The convention applies symmetrically to `INVESTIGATION.md` and
any sibling created by `/fix-issue`.

### 10. Completeness summary (the gate)

Every NEW finding must reach a terminal action — created, skipped-as-duplicate, or
skipped-with-reason. Print the table and assert the count matches: NEW findings parsed ==
(Created + Skipped). A NEW finding that is neither created nor consciously skipped is a
publish bug.

| Finding | Action | Reason |
|---------|--------|--------|
| REN-001 | Created #42 | NEW, CONFIRMED |
| REN-002 | Skipped | Existing #38 |
| REN-003 | Skipped | STALE (fixed in `composite.rs`) |
| BSA-004 | Created #43 | NEW, CONFIRMED — labeled `import-pipeline` (no `bsa` label) |

Flag any subsystem-without-a-label mappings (step 6) here so the gap is visible.

### 11. Suggest next step

For each created issue:

```
Fix with: /fix-issue <number>
```
