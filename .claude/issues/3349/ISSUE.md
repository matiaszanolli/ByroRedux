# FNV-2026-08-26-D8-06

**Issue**: #3349
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 8 — Real-Data Validation & Bench
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/commands/assets.rs:17-69`

**Premise verified**: `TexMissingCommand::execute` iterates `TextureHandle` and buckets
only `tex.0 == 0`. `TextureHandle` is the single base-color handle; the 26-slot
`MaterialTextureHandles` (`byroredux/src/components.rs:296-304`,
`MaterialTextureSet<u32>`) is imported into `assets.rs:6` but read only by `mesh.info`
(`assets.rs:266`, and only for `greyscale_lut`). No scene-wide command rolls up
per-slot fallbacks; `mat.dump` (`commands/scene.rs:567`) is per-entity only.

**Impact**: the FNV "chrome / posterized surfaces → run `tex.missing` first" triage
loop documented in CLAUDE.md and `SKILL.md:146` dead-ends whenever the *normal*,
specular, environment or glow slot is the one that fell back — `tex.missing` reports
0 and the operator has no scene-wide signal. It does not bite the two cells measured
here (both resolve every authored slot — see below), so severity is LOW: this is
diagnostic coverage, not a rendering defect.

**Fix sketch**: extend the bucket loop to walk `MaterialTextureHandles.textures`,
reporting `<path> [slot=normal]` etc.; the per-role paths already exist on `Material`,
and `MaterialTextureSource` (`components.rs:316`) already carries the provenance
needed to suppress "absent by design" roles.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
