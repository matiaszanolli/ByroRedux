# #3874: TD4-2026-09-05-05: seven dead backticked bare basenames sit in the skill tier itself, invisible to the gate per #3439 — and one of them makes `audit-incremental` state a fact that is wrong for three of its four names

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-05) via `/audit-publish`, 2026-09-05. Labels: `low,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3874 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-05), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-common.md:90`; `.claude/commands/audit-renderer/SKILL.md:34`, `:209`; `.claude/commands/audit-fo4/SKILL.md:157`, `:160`; `.claude/commands/audit-oblivion/SKILL.md:188`; `.claude/commands/audit-incremental/SKILL.md:96`
- **Status**: NEW (the *content* backlog; the gate gap itself is **Existing: #3439**, not re-filed)
- **Effort**: trivial (≤30 min)

**Description**
**#3439's premise re-verified and it still holds**, with one correction worth
recording: the issue's headline evidence was `grep -c '`ai\.rs' docs/engine/npc-spawn-ai-packages.md` → **10**; today that file carries **1**. The
*structural* gap is unchanged — `should_skip()`'s first rule still discards
every ref without a `/`, and `git ls-files | grep -E "(^|/)ai\.rs$"` still
returns nothing, so the surviving citation still passes silently. Do not treat
the shrunken count as the issue being fixed; the blind spot is intact.

Enumerating what that blind spot hides **inside my own scope** (the skill tier,
which #3439's evidence did not measure) finds seven dead refs:

| Site | Ref | Reality |
|---|---|---|
| `_audit-common.md:90` | ``fire_lights.rs`` | Deleted `2325c1de` (2026-08-17) — and the sentence **says so**: *"**`fire_lights.rs` was deleted 2026-08-17**"* |
| `audit-renderer/SKILL.md:209` | ``references.rs`` | Split into `byroredux/src/cell_loader/references/` (a directory: `attach.rs`, `complete.rs`, `import.rs`, `synth_child.rs`, `mod.rs`); no `references.rs` exists anywhere |
| `audit-fo4/SKILL.md:157`, `:160` | ``references.rs`` ×2 | same |
| `audit-renderer/SKILL.md:34` | ``render.rs`` | Deleted; the note is *"`byroredux/src/render/` is a **directory**, not a `render.rs` file"* |
| `audit-incremental/SKILL.md:96` | ``render.rs`` | same |
| `audit-oblivion/SKILL.md:188` | ``actor.rs`` | *"split from `actor.rs`, #2055"* — backwards-looking |

Two distinct convention faults are mixed here. `_audit-common.md:90`,
`audit-renderer:34` and `audit-oblivion:188` are **negative/backwards-looking
sentences that backtick the absent name** — the same fault as
TD4-2026-09-05-04, one tier up. `_audit-common.md:90` is the sharpest instance:
the same sentence correctly italicises the three deleted *symbols*
(*derive_fire_light* / *fire_lights_enabled* / *BYRO_FIRE_LIGHTS*) and then
backticks the deleted *file*. The convention was known and applied unevenly
within one sentence.

The three ``references.rs`` sites are the other fault — **positive assertions
about a file that no longer exists**:

```
audit-fo4/SKILL.md:157   `references.rs` fires the first matching expander and composes placements.
audit-fo4/SKILL.md:160   (`cell_loader/spawn.rs` + `references.rs`, cached in `nif_import_registry.rs`)
audit-renderer/SKILL.md:209  `references.rs` `debug_assert!`s loaded-cell max |coord| stays under it
```

**`audit-incremental/SKILL.md:96` additionally states something false.** Its
sentence is *"`render.rs`, `systems.rs`, `scene.rs`, and `cell_loader.rs` are
all directories now"*. Three of the four are still real files with a directory
sibling:

```
ABSENT: byroredux/src/render.rs
EXISTS: byroredux/src/systems.rs      (54 LOC)
EXISTS: byroredux/src/scene.rs        (1706 LOC)
EXISTS: byroredux/src/cell_loader.rs  (580 LOC)
```

This is a "layout shifts that often surprise a delta audit" callout — the one
paragraph whose entire job is to stop an auditor being surprised by layout.

**Impact**
Low blast radius (documentation), but it lands on the gate's *own* corpus, in
the paragraph meant to prevent layout surprise, and it demonstrates that
#3439's hole is not confined to `docs/engine/` — the tier it was measured in.
Each dead ``references.rs`` sends an FO4/renderer auditor to a path that does
not resolve.

**Related**
**Existing: #3439** (OPEN — the gate blind spot; deliberately not re-filed).
#1189 (CLOSED — *12 stale `byroredux/src/render.rs` refs in 5 audit skill
files*; that sweep fixed the **qualified** form, and these bare-basename
survivors are exactly what the gate could not see afterwards).
#1229 (CLOSED — same pattern for `tri_shape.rs`).

**Suggested Fix**
Repoint each ``references.rs`` site at its *actual* home — they are three
different files, so a blanket rewrite to `references/` would be wrong twice:

| Site | Correct target |
|---|---|
| `audit-renderer:209` (RT precision ceiling) | `byroredux/src/cell_loader/references/complete.rs:82` holds the assert; the constant and `worldspace_extent_over_rt_ceiling` are exported from `references/mod.rs` and pinned in `references/import_tests.rs` |
| `audit-fo4:157` (SCOL/PKIN expanders) | **`byroredux/src/cell_loader/refr.rs`** — `expand_scol_placements` / `expand_pkin_placements` never moved into `references/` |
| `audit-fo4:160` (attach-graph materialization) | `byroredux/src/cell_loader/references/attach.rs`, with `spawn.rs` and `nif_import_registry.rs` as the other two links; the cited `cell_loader/attach_points_spawn_tests.rs` does exist |

Then italicise *fire_lights.rs*, *render.rs* and *actor.rs* at the four
backwards-looking sites, and rewrite `audit-incremental:96` to name only
`render.rs` as gone while describing `systems.rs` / `scene.rs` /
`cell_loader.rs` as thin dispatch files beside their directories. Landing
#3439's two-line fix afterwards keeps the class from returning.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
