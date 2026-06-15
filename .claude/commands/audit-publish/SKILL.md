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

Every label this skill applies MUST appear in that file. The mapping below reflects how
the repo is *actually* labeled (verified against label-usage counts across all issues) —
**not** the broader vocabulary in _audit-common's "Domain Labels" list, several of which
have no label in this repo.

**Severity** (always exactly one, all exist): `critical` · `high` · `medium` · `low`.

**Domain** (zero or more; only these exist as labels):
`ecs` · `renderer` · `vulkan` · `pipeline` · `memory` · `sync` · `cxx` · `nif` ·
`nif-parser` · `import-pipeline` · `animation` · `legacy-compat` · `performance` ·
`safety` · `tech-debt` · `info`.

**Type** (one): `bug` · `enhancement` · `documentation`. There is **no** `maintenance`
label — tech-debt findings use the `tech-debt` domain label plus `bug` (or `documentation`
for doc-rot), never `maintenance`.

**Domain mapping for subsystems with no own label** — map to the closest existing label
and note the gap in the summary; never invent a label:

| Finding subsystem | Apply (exists) | NOT (does not exist) |
|-------------------|----------------|----------------------|
| NIF parser / block dispatch | `nif-parser` (primary) `nif` (format tag) | — |
| BSA / BA2 / CSG archive readers | `import-pipeline` (or `nif-parser`) | `bsa` |
| ESM / cell / plugin loading | `import-pipeline` + `legacy-compat` | `esm` |
| Audio (M44) | `legacy-compat` / `tech-debt` per finding | `audio` |
| SpeedTree / sfmaterial / debug-ui / facegen / physics / platform | closest of `renderer` / `import-pipeline` / `legacy-compat` | `spt` `sfmaterial` `debug-ui` `platform` |
| NIFAL canonical-translation | the subsystem it lives in (`nif-parser` / `renderer`) + cross-link `/audit-nifal` | — |

If a finding genuinely has no reasonable existing label, file it with severity + `bug`
only and flag the missing-label gap in the summary. Do **not** `gh label create` ad hoc —
new labels are a deliberate repo decision, not a per-publish side effect.

**Report-family defaults** — the `AUDIT_<TYPE>_<DATE>.md` filename selects a default
domain/type; a per-finding `Dimension`/domain always overrides it:

| Report (`AUDIT_<TYPE>_*.md`) | Default domain | Type | Extra |
|------------------------------|----------------|------|-------|
| `AUDIT_RENDERER_*` | `renderer` | `bug` | — |
| `AUDIT_ECS_*` | `ecs` | `bug` | — |
| `AUDIT_CONCURRENCY_*` | `sync` | `bug` | — |
| `AUDIT_NIF_*` / `AUDIT_NIFAL_*` | `nif-parser` | `bug` | — |
| `AUDIT_FNV_*` / `AUDIT_FO3_*` / `AUDIT_FO4_*` / `AUDIT_SKYRIM_*` / `AUDIT_OBLIVION_*` / `AUDIT_STARFIELD_*` | `nif-parser` (+ per finding) | `bug` | `legacy-compat` |
| `AUDIT_LEGACY_COMPAT_*` | `legacy-compat` | `bug` | — |
| `AUDIT_AUDIO_*` | (per finding; `legacy-compat`) | `bug` | — |
| `AUDIT_TECH_DEBT_*` | (per finding) | `bug` | `tech-debt` |
| `AUDIT_INCREMENTAL_*` | (per finding) | `bug` | — |

For `AUDIT_TECH_DEBT_*` the final set is `<severity>,<domain?>,tech-debt,bug` (doc-rot
findings swap `bug` → `documentation`). For everything else: `<severity>,<domain>,bug`.

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
