# #1606 — NIF-D5-05: Starfield LOD BSLightingShaderProperty under-reads +38 B

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: MEDIUM (stream-position mismatch the `block_size` reconciliation covers; Starfield is a stretch target, LOD only) · **Dimension**: Collision/Shader Parsing (shader tail) · **Status**: NEW
**Source**: AUDIT_NIF_2026-06-14 (NIF-D5-05)
**Game Affected**: Starfield (bsver 172) — `Starfield - LODMeshes.ba2`.

**Location**: [blocks/shader.rs](crates/nif/src/blocks/shader.rs) `parse_fo76_plus`, the Starfield branch past the #1510 `bsver < STARFIELD` gate (~`:1118`).

## Description
26 Starfield LOD shaders consume 38 B fewer than their `block_size` — a Starfield-specific trailing field the FO76+ parser doesn't read (likely a material/wetness-tail extension). Positive drift (under-read), so it is **not** the #1510 over-read regression — #1510 is confirmed fixed (`bsver < STARFIELD` correctly excludes Starfield from the FO76 luminance/translucency/texture-array tail). `block_size`-absorbed; 0 `NiUnknown`.

## Evidence
`nif_stats "Starfield - LODMeshes.ba2" --drift-histogram` → `BSLightingShaderProperty drift=+38×26`; same archive `--tsv` → 48,903 parsed / 0 unknown.

## Impact
26 Starfield LOD shaders parse with a 38-byte trailing field defaulted (the LOD material loses whatever that field carries). Bounded.

## Related
06-13 NIF-NEW-05; #1510.

## Suggested Fix
Byte-audit the Starfield (bsver ≥ 172) `BSLightingShaderProperty` tail against nif.xml `#STARFIELD#` conditionals; add the missing trailing field(s) to the Starfield path of `parse_fo76_plus`.

## Completeness Checks
- [ ] **SIBLING**: Check `parse_fo76_plus`'s `BSEffectShaderProperty` Starfield tail for the same missing-trailing-field pattern
- [ ] **TESTS**: A regression test pins `BSLightingShaderProperty` consumed == `block_size` on a traced Starfield LOD instance
