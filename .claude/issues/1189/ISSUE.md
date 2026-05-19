# #1189 — TD7-NEW-01 / TD10-NEW-01: 12 stale `byroredux/src/render.rs` refs in audit skill files

**Severity**: LOW
**Dimension**: Stale Documentation / Audit-Finding Rot
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-05-19.md`
**Created**: 2026-05-19
**Effort**: trivial (~10 min sed sweep)

## Sites (12 across 5 files)

- `.claude/commands/audit-fo3.md:66`
- `.claude/commands/audit-incremental.md:43`
- `.claude/commands/audit-performance.md:51, 77, 87`
- `.claude/commands/audit-renderer.md:194, 211, 220, 241, 258, 333`
- `.claude/commands/audit-safety.md:52`

All reference `byroredux/src/render.rs` which was renamed to `byroredux/src/render/mod.rs` in commit `1164917d` (#1115 Step 1, 2026-05-15). Steps 2-8 extracted submodules into `byroredux/src/render/{lights,sky,water,particles,camera,skinned,static_meshes}.rs`.

## Fix recipe

Single sweep commit. For each cited line, replace `byroredux/src/render.rs` with either:
- `byroredux/src/render/mod.rs` — if the ref is about top-level dispatch (e.g., `build_render_data`)
- `byroredux/src/render/` — if the ref is about the rendering module as a whole (most cases)

Then run `.claude/commands/_audit-validate.sh` and confirm 0 stale refs.

## Why this matters

Today's `/audit-renderer` run dispatched dim-agents with the stale path in their prompts. The validate gate (#1114) flagged the staleness in parallel, but the prompts were already out the door. Two of three agents wasted tool budget on `git log` archaeology.

## Related

- #1114 — the validate gate that catches this class
- #1115 — the refactor that introduced the staleness

## Next step

```
/fix-issue 1189
```
