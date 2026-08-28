# #3422 — FNV-2026-08-27-D1-02: the /audit-fnv skill's Dimension-1 checklist still sends auditors after `_far.nif`

**Labels**: low, documentation, doc-rot, tech-debt, terrain-exterior, game:fnv, legacy-compat

**Filed**: 2026-08-27 · from `docs/audits/AUDIT_FNV_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-27.md` — finding `FNV-2026-08-27-D1-02` (HEAD `969d81c8`)

- **Severity**: LOW
- **Dimension**: audit infrastructure (doc rot)
- **Location**: `.claude/commands/audit-fnv/SKILL.md:58` — Dimension 1 checklist, final bullet

## Description

The checklist reads *"`_far.nif` distant-object LOD (#1726/#1745, Session 52) — verify the placement scheme + real LOD textures still resolve on FNV's WastelandNV exterior grid; entry points `cell_loader/object_lod.rs`, `cell_loader/placement_lod.rs`."* FNV ships no `_far.nif` and no `distantlod\` at all, and `placement_lod_supported` is Oblivion-only by construction, so the check can only ever confirm a no-op. Meanwhile the scheme FNV *does* ship — `ObjectLodScheme::FalloutLegacyBlocks`, landed 2026-08-27 under #3321 — has no checklist coverage.

## Evidence

Census over all 20 FNV BSAs (182 177 entries) this audit: **0 `_far.nif`, 0 `distantlod\` entries.** And `byroredux/src/cell_loader/placement_lod.rs:313-315`:

```rust
pub(crate) fn placement_lod_supported(game: GameKind) -> bool {
    game == GameKind::Oblivion
}
```

pinned by `placement_lod_supported_is_oblivion_only` (`:754-761`), which asserts `!placement_lod_supported(GameKind::Fallout3NV)`. The live FNV scheme is `object_lod.rs:458-467` / `:491-497` / `:508-514`.

## Impact

Wasted auditor effort on a guaranteed-empty check, and — the costlier half — no standing instruction to verify the newest, least-reviewed FNV LOD code. Same class as FNV-2026-08-26-D8-01 (#3331), whose D8 repro command names archives a vanilla install does not have.

## Related

#3321 (`e23a9908`), #2086, #3331, #1726, `docs/engine/exal.md` §5.

## Suggested Fix

Replace the bullet with the `FalloutLegacyBlocks` checks that are actually verifiable — quad path shape (`meshes\landscape\lod\<world>\blocks\<world>.level<L>.x<qx>.y<qy>.nif`), the shared `<world>.buildings.dds` atlas resolving out of `Fallout - Textures2.bsa`, and the `LodBandLadder::for_object_game` legacy-ladder arm — and keep one line recording that `_far.nif` / `distantlod\` are confirmed absent on FNV so nobody re-derives it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the FO3 skill's Dimension-1 checklist shares the `_far.nif` premise; Oblivion's is the one game where it is real)
- [ ] **TESTS**: A regression test pins this specific fix (n/a for a skill doc — the pin is the census line recorded in the checklist)
