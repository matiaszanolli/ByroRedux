# #3524 — SF-2026-08-27b-D7-01: the six residual MeshesPatch truncations are all BSWeakReferenceNode — the #2105 tail is characterised, not unexplained

Source: `docs/audits/AUDIT_STARFIELD_2026-08-27b.md`
Filed: 2026-08-28 (`/audit-publish`)
Labels: medium, bug, nif-parser, nif, game:starfield, legacy-compat

---

From `docs/audits/AUDIT_STARFIELD_2026-08-27b.md` (branch `main` @ `969d81c8`).

- **Severity**: MEDIUM
- **Dimension**: 7 — Real-data validation (root cause in Dim 6's block-parser territory)
- **Location**: `crates/nif/src/blocks/node.rs` — `BsWeakReferenceNode::parse_inner` (the `SF_WEAK_REF_GAP` 2-byte skip, `unk_int1`, and the water-reference loop that follows it)

## Description

Two prior audits recorded the six residual `Starfield - MeshesPatch.ba2` truncations as a stable-but-unexplained family, explicitly "distinct unexplained cause from the closed `BSWeakReferenceNode` / cloth / `BSShaderType155` tails". They are not distinct. **All six are `BSWeakReferenceNode`**, all at `user_version_2 == 175` (i.e. at-or-above `SF_WEAK_REF_GAP`, so the #2105 2-byte skip *is* applied), and all six drop to `NiUnknown` inside the same water-reference loop.

This is the remainder of #2105's fix, not a regression of it.

## Evidence

Measured against `Starfield - MeshesPatch.ba2`:

| File | block | size | consumed | failure |
|---|---|---|---|---|
| `meshes\terrain\cydoniacity\objects\cydoniacity.4.-2.-2.nif` | 0 | 150 324 | 150 314 | `skip(80)` past EOF |
| `meshes\terrain\sb004templeworld\objects\sb004templeworld.1.-1.0.nif` | 1 | 14 764 | 14 754 | `skip(80)` past EOF |
| `meshes\terrain\lc174world\objects\lc174world.1.0.1.nif` | 1 | 208 | 174 | `skip(1634533376)` |
| `meshes\terrain\cydoniacity\objects\cydoniacity.8.-6.-6.nif` | 0 | 302 052 | 302 042 | `skip(80)` past EOF |
| `meshes\terrain\cydoniacity\objects\cydoniacity.1.-1.-1.nif` | 1 | 14 284 | 14 274 | `skip(80)` past EOF |
| `meshes\terrain\cydoniacity\objects\cydoniacity.2.-2.-2.nif` | 0 | 35 860 | 35 850 | `skip(80)` past EOF |

1. **Five of six stop at exactly `block_size − 10`.** `80 = 64 + 12 + 4` is the water-reference struct skip, so the parser read `num_water_refs` as a non-zero garbage value 10 bytes before the block end.
2. **The 10 bytes are byte-regular across files.** Hex at the block tail, immediately after the last weak-ref entry's `num_materials = 0`:
   ```
   cydoniacity.1.-1.-1.nif   01 00  33 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
   cydoniacity.2.-2.-2.nif   01 00  6a 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
   cydoniacity.4.-2.-2.nif   01 00  d8 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
   ```
   `u16 = 1`, then a per-file-varying `u16`, then the **constant** `u32 0x001AC0CB` (1 753 291) in all three sampled files, then two zero bytes. The `[2-B gap][unk_int1 = 0][num_water_refs = 0]` triple then lands exactly on the block end, which is self-consistent.
3. **A clean sibling proves the run is conditional, not universal.** `meshes\terrain\cydoniacity\objects\cydoniacity.4.-6.2.nif` — same directory, same `user_version_2 = 175`, same `BSWeakReferenceNode` — has only `[gap 2][unk_int1 4][num_water_refs 4] = 10` bytes after `num_materials`, no 10-byte run, and parses clean.
4. **The sixth file diverges earlier.** `lc174world.1.0.1.nif` attempts `skip(1634533376)`; `1634533376 == 0x616D0000`, i.e. the ASCII bytes `\0 \0 m a` — the parser is misaligned *inside* a `materials\…` null-terminated string in the `UnkMaterialStruct` loop (`read_past_cstring`). A distinct sub-mode of the same block type, worth separating in any fix.

**Attempts to disprove** (all failed): the files are not corrupt (the BA2 extract succeeds and the header `block_sizes` sum + 8-byte footer accounts for the file exactly); the #2105 gate is not mis-applied (all six are bsver 175, the gate's own attested boundary, and removing the skip moves the misread *further* off); the outer recovery is working as designed (`truncated == false`, `dropped_block_count == 0`, only `recovered_blocks == 1`), so this is content loss, not stream corruption.

## Impact

Six `BSWeakReferenceNode` blocks — Starfield's composite-LOD / packin reference nodes — are replaced with `NiUnknown`, so their entire weak-reference payload (the terrain-object LOD placements) is dropped. Four of the six are **Cydonia** terrain-object LOD tiles, i.e. the flagship walkable cell. Blast radius is bounded and non-fatal (6 / 29 849 files; the parse-rate gate at 99.5% still passes at 99.98%), but the family is now actionable rather than mysterious.

## Related

#2105 (the 325 → 6 fix), #2201 (its `SF_WEAK_REF_GAP` correction), #1882 (the +2 B opaque tail on the same block), #746/#747 (the original mis-attribution this closes out).

## Suggested Fix

Do **not** guess the field's semantics. Two safe steps:
1. Make the water-reference loop defensive — bail to the block-size boundary rather than issuing a `skip()` that provably exceeds it, so the block keeps its `NiNode` base and children instead of collapsing to `NiUnknown`.
2. Byte-audit the 10-byte run against nifly's `BSWeakReference` to determine whether it is a conditional per-entry field or a block-level one, using the clean/failing sibling pair above as the differential.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
