# Investigation — #1239 (NiPSysEmitter base under-read on Oblivion)

## Reconciliation of the contested premise

The audit agent's finding contradicted a load-bearing comment at
`crates/nif/src/blocks/particle.rs:47-63` written during #383's
closure: *"Oblivion (BSVER 11, version 20.0.0.5): 12 floats (the
BS_GTE_FO3 gate keeps these 2 floats out, so we don't over-read and
shift downstream blocks into garbage)."* The comment cited audit D5-F2
and #383 as the empirical basis.

Per [feedback_audit_findings.md](../../../../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_audit_findings.md),
verified the premise before editing code.

### Empirical evidence — pre-fix

Ran `cargo run --release -p byroredux-nif --example trace_block -- "Oblivion - Meshes.bsa" meshes/magiceffects/fireball.nif`:

```
[167] @  37751  NiPSysAgeDeathModifier   consumed=34
[168] @  37785  BSPSysArrayEmitter       consumed=68     ← agent's claim
[169] @  37853  NiPSysSpawnModifier      [ERR: 1 065 353 216-byte alloc]
```

`1065353216 = 0x3F800000 = 1.0f32` — the missing float reinterpreted
as a u32 length. Hex peek of block [169] confirmed the string
`"NiPSysSpawnModifier:1"` (decimal 21 = `0x15`) starts at offset
`37853 + 8 = 37861`, so `BSPSysArrayEmitter` should have consumed
**76** bytes, not 68. **Agent's evidence reproduced exactly.**

Pre-fix `Oblivion - Meshes.bsa` sweep: 97.27% clean / 219 truncated /
15 182 dropped blocks.

### nif.xml authoritative gate

`/mnt/data/src/reference/nifxml/nif.xml:3589`:

```xml
<field name="Radius Variation" type="float" since="10.4.0.1">
```

Oblivion's version 20.0.0.4/5 trivially satisfies `>= 10.4.0.1`.
The pre-#1239 BSVER gate (`>= 34` = BS_GTE_FO3) was empirically
narrower than nif.xml's version gate.

### Why #383's "Oblivion parsed cleanly" claim missed this

#383 was driven by FNV warning counts (5,837 → 1,274 = 78% reduction)
and validated `Oblivion parsed cleanly` at the aggregate parse-rate
level. But:

- Most Oblivion meshes are static architecture / clutter / NPC bodies —
  zero NiPSys* content
- Oblivion v20.0.0.5 has **no `block_sizes` table** for recovery —
  any per-particle-NIF drift is a hard truncation
- The 219 truncated NIFs are concentrated in
  `meshes\fire\`, `meshes\magiceffects\`, `meshes\effects\`,
  `meshes\dungeons\misc\fx\` — the rare-but-important particle effect
  content the aggregate count didn't weight

So #383's empirical claim held at the wrong granularity. Both
empirical claims (#383's "Oblivion clean" and this audit's "Oblivion
under-reads by 8") are true — they're measuring different populations.

## Fix

Single-line gate change at `crates/nif/src/blocks/particle.rs:64`:

```rust
// Before: if stream.bsver() >= 34 {
if stream.version() >= NifVersion::V10_4_0_1 {
    stream.skip(4 * 2)?;
}
```

Plus comment rewrite documenting the #383 reconciliation.

## Post-fix verification

| BSA | Pre | Post |
|-----|-----|------|
| `Oblivion - Meshes.bsa` | 97.27% / 219 truncated | **97.55% / 197 truncated** |
| `Fallout - Meshes.bsa` (FNV) | 100.00% / 0 truncated | **100.00% / 0 truncated** |
| `Fallout - Meshes.bsa` (FO3) | 100.00% / 0 truncated | **100.00% / 0 truncated** |
| `Skyrim - Meshes0.bsa` (SSE) | 100.00% / 0 truncated | **100.00% / 0 truncated** |

22 Oblivion files fixed by this single change. **No regression of
#383 on FNV/FO3/Skyrim+.** The remaining 197 truncated Oblivion
files involve different blocks (`NiPSysDragModifier`,
`NiPSysGrowFadeModifier`, `NiPSysMeshEmitter`, `NiPSysBoxEmitter`)
— distinct under-read bugs outside #1239's scope. Trace_block on
`fireball.nif` post-fix confirms `BSPSysArrayEmitter` reads 76 bytes
correctly and `NiPSysSpawnModifier` reads 60 bytes cleanly; failure
moves to `NiPSysDragModifier` at offset 38189 (different bug).

Agent's predicted impact ("219 → 0 truncated") was optimistic — the
8-byte under-read cascade was masking *some* of the downstream
truncation but not all of it. Real improvement: 22 files / 2.18 k
blocks recovered, plus 665 new `recovered` (partial-unknown) entries
where the fix kept the parser on track far enough to reach the next
distinct bug instead of cascading immediately.

## Regression test

`parse_sphere_emitter_consumes_full_block_oblivion` at
`particle.rs:1320-1380`. Uses a new `modifier_base_bytes_oblivion`
fixture (length-prefixed inline string instead of string-table index,
since Oblivion v20.0.0.4 is below `STRING_TABLE_THRESHOLD`).
Asserts 77 bytes consumed for the full
modifier+emitter+volume+radius chain. Sibling to the existing
`parse_sphere_emitter_consumes_full_block` FNV regression test.

## What the agent got right vs wrong

- **Right**: the gate is wrong; nif.xml's version gate is the correct one; the trace_block evidence is exact; FNV/FO3/SSE don't regress.
- **Approximately right (overstated)**: impact estimate — fix recovers 22 files / 2.18 k blocks, not the predicted "near-0 truncated / ~99-100% clean." The cascade-masking effect was real but the remaining truncations belong to sibling NiPSys parsers with their own bugs.
- **Wrong**: claim that 219 truncations all traced to this one bug. Many were already trapped by other under-reads downstream.

Net: a real HIGH finding with a real fix, but the next 197 truncated
Oblivion files need their own audit pass — likely the same flavor of
nif.xml-vs-BSVER gate drift on the other NiPSys* parsers
(`parse_drag_modifier`, `parse_grow_fade_modifier`, `parse_mesh_emitter`,
`parse_box_emitter`).
