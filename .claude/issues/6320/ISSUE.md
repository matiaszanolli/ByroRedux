# #6320: FNV-2026-10-21-04: FO3/FNV hands restored after an outfit is taken skip the spawn-time seam blend (known in the commit, untracked)

**Labels**: low,bug,game:fnv,game:fo3,legacy-compat,nifal,gameplay

**Source**: docs/audits/AUDIT_FNV_2026-10-21.md · **Severity**: LOW · **Dimension**: 4 (spawn seam blend). **Owner**: `/audit-gameplay` / `/audit-nifal`. · **Location**: `byroredux/src/npc_spawn/loot_appearance.rs` (`RestorePart` re-import); `byroredux/src/npc_spawn/resumable/runtime.rs:700-713` / `:1124-1135` (`blend_part_seams` runs only at spawn).

- **Severity**: LOW
- **Dimension**: 4 (spawn seam blend). **Owner**: `/audit-gameplay` / `/audit-nifal`.
- **Location**: `byroredux/src/npc_spawn/loot_appearance.rs` (`RestorePart` re-import); `byroredux/src/npc_spawn/resumable/runtime.rs:700-713` / `:1124-1135` (`blend_part_seams` runs only at spawn).
- **Status**: NEW. The `1a8c05139` (#6128) message says "FO3/FNV restored hands still lose the wrist-cut seam blend: it needs the live neighbours' tone context and is minor". No issue was filed.
- **Impact**: After looting gloves or a body outfit from an FNV corpse or NPC, the re-imported hands show the hard wrist seam that the spawn path hides. Cosmetic only.
- **Suggested Fix**: Keep the part's `SeamContext` (`RuntimeNpcState::seam_context` is already an `Arc`) on the `RestorePart`, and run `blend_part_seams` on the restored part.

**Publish note**: residual explicitly left by the #6128 fix (`1a8c05139` commit message), not a regression of #6128.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_FNV_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
