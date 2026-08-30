# #3615 — REN-2026-08-30-D17-04: `ImportedMaterial::lighting_effect_2` is documented as the Skyrim *backlight* scalar; `nifly` — the reference checked into `/mnt/data/src/reference/` — names the same wire field `rimlightPower`, which is what the shader actu...

**Labels**: `low,renderer,nif-parser,doc-rot,documentation,game:skyrim`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3615 --json state`.

---

- **Severity**: LOW
- **Dimension**: Disney BSDF (Bethesda lighting-response family — doc vs. consumer contradiction)
- **Location**: `crates/nif/src/import/material/mod.rs` (doc block, lines 643-647), `crates/nif/src/blocks/shader.rs` (`parse_skyrim`, lines 927-928), `crates/renderer/shaders/include/lighting.glsl` (`bethesdaRimFactor` line 98, `bethesdaBackFactor` line 106). Reference: `/mnt/data/src/reference/nifly/include/Shaders.hpp:647-648`, `/mnt/data/src/reference/nifly/src/Shaders.cpp:468-471`, `/mnt/data/src/reference/nifxml/nif.xml:6605-6609`
- **Status**: NEW. Distinct from #3448 (which is about `bethesdaRimFactor`'s `0.0 → 0.25` clamp floor) and from #3452 (FO4 `FLT_MAX` sentinel). This is the *identity* of the Skyrim fallback field, not its clamping.
- **Description**: The ByroRedux doc block says:

  > `BSLightingShaderProperty.lighting_effect_2` — Skyrim backlight scalar (BSVER < FO4, gated by `SLSF2_Back_Lighting`). Drives the back-lit translucency term on hair / foliage / fabric edges. Default 0.0 = no backlight.

  `nifly` reads the same two floats at the same offsets for the same version window (`stream.GetVersion().User() <= 12 && Stream() < 130`) and names them `softlighting` (default `0.3f`) and **`rimlightPower` (default `2.0f`)** — `Shaders.hpp:647-648`, `Shaders.cpp:468-471`. `nif.xml` agrees on the defaults (`Lighting Effect 1` default `0.3` range `0..10`; `Lighting Effect 2` default `2.0` range `0..1000`). So slot 2 is the **rim-light power**, and Skyrim has no authored backlight strength at all.

  The shader already implements it correctly:
  `bethesdaRimFactor` uses `exponent = mat.rimlightPower > 0.0 ? mat.rimlightPower : mat.lightingEffect2;` (lighting.glsl:100-102) — the FO4 field first, the Skyrim field as the fallback — and `bethesdaBackFactor` deliberately does **not** read `lightingEffect2`, with the correct in-code justification *"Skyrim's slot-7 back-light map has no separate strength scalar"* (lighting.glsl:108-110). The importer doc is the only thing that is wrong, and it contradicts the consumer it feeds.
- **Evidence**:
  - `nifly` field order and version gate: `Shaders.cpp:468-471` — `if (User() <= 12 && Stream() < 130) { Sync(softlighting); Sync(rimlightPower); }`. ByroRedux `parse_skyrim` reads `lighting_effect_1` then `lighting_effect_2` at the same position (`crates/nif/src/blocks/shader.rs:927-928`) — same two floats, so the mapping is 1:1.
  - `nifly` public accessors confirm the semantics: `GetRimlightPower()` returns `rimlightPower`, `GetSoftlight()` returns `softlighting`, `GetBacklightPower()` returns the FO4-only `backlightPower` (`Shaders.cpp:668-680`).
  - Secondary, same doc block: `lighting_effect_1`'s *"Default 0.0 = no SSS contribution"* is not the format default either — nifly and nif.xml both ship `0.3`. Harmless in practice (`parse_skyrim` always reads the wire value, so the struct default is only reached by non-BSLSP materials whose `SOFT_LIGHTING` bit is clear anyway), but it makes the doc a bad source for anyone reasoning about unauthored materials.
- **Impact**: A reader who trusts the doc will "fix" the shader — either by moving `lightingEffect2` from `bethesdaRimFactor` into `bethesdaBackFactor`, or by re-gating it on `MAT_FLAG_BACK_LIGHTING` — and break the Skyrim rim path, which is currently correct. This family already produced two filed defects (#3448, #3452) in five days; a doc that misidentifies one of its three Skyrim-reachable fields is an active trap for the next fix in the same file.
- **Suggested Fix**: Rewrite the two doc blocks at `crates/nif/src/import/material/mod.rs:638-647` to match `nifly`: `lighting_effect_1` = Skyrim soft-lighting / subsurface width (nifly `softlighting`, format default 0.3, gated by `SLSF2_Soft_Lighting`), `lighting_effect_2` = Skyrim rim-light power (nifly `rimlightPower`, format default 2.0, gated by `SLSF2_Rim_Lighting`), and state explicitly that Skyrim authors **no** backlight strength — which is why `bethesdaBackFactor` uses a unit fallback. Cite `nifly Shaders.cpp:468-471` inline so the next reader does not have to re-derive it. Doc-only; no shader change (the shader is right).

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D17-04

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
