# #3613 — REN-2026-08-30-D17-03: the Disney anisotropic lobe (#1250/#1254) is unreachable from every importer, and the code comment that justifies it ("no BGSM/BGEM/inline-NIF field maps to them") is only half true — the *enable* bit exists in both formats,...

**Labels**: `low,renderer,shaders,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3613 --json state`.

---

- **Severity**: LOW
- **Dimension**: Disney BSDF (coverage / stale rationale)
- **Location**: `byroredux/src/material_translate.rs` (lines 575-581, `anisotropic: 0.0`), `crates/renderer/shaders/include/pbr.glsl` (`distributionGGXAniso` line 41, `deriveAxAy` line 71), `crates/renderer/shaders/include/lighting.glsl` (aniso branch, lines 189-206). Contradicting evidence: `crates/bgsm/src/bgsm.rs` (`aniso_lighting` field line 88, parsed line 254), `crates/nif/src/shader_flags.rs` (`skyrim_slsf2::ANISOTROPIC_LIGHTING` line 149, `fo4_slsf2::ANISOTROPIC_LIGHTING` line 256)
- **Status**: NEW (recast of a coverage gap, not a re-file — no `anisotrop` hit in `issues.json` or in `AUDIT_RENDERER_2026-08-27.md`)
- **Description**: `translate_material` hardcodes `anisotropic: 0.0` under the comment *"Disney-BSDF-only parameters with no source-format equivalent (no BGSM/BGEM/inline-NIF field maps to them) … Reachable only via `mat.set` (Cornell harness)."* The "reachable only via `mat.set`" half is accurate and verifiable: `grep -rn "anisotropic" --include=*.rs byroredux/src crates/` shows the only writers of `Material::anisotropic` are `byroredux/src/commands/scene.rs:963` (the `mat.set` console command) and `byroredux/src/cornell.rs:1495`. Every importer path leaves it at `0.0`, so the `mat.anisotropic > 0.0` branch in `shadowableLightRadiance` (lighting.glsl:189) never taken on loaded game content, and `distributionGGXAniso` / `deriveAxAy` execute only in the Cornell box.

  The "no source-format equivalent" half is wrong as written. `BgsmFile::aniso_lighting` is a parsed `bool` (bgsm.rs:254), and both `skyrim_slsf2` and `fo4_slsf2` define `ANISOTROPIC_LIGHTING = 0x0020_0000`. What no format supplies is a *strength scalar* — which makes the current `0.0` the right call under the no-guessing policy, but for a different reason than the comment gives.
- **Evidence**:
  - `byroredux/src/asset_provider/material.rs:1652` lists `aniso_lighting` in its own inventory of BGSM fields that are decoded but not forwarded — so the field's existence is already known one module away from the comment that denies it.
  - `crates/nif/src/shader_flags.rs:412` asserts `fo3nv_f2::ALPHA_DECAL == skyrim_slsf2::ANISOTROPIC_LIGHTING` — the same bit means two different things across families.
  - The #1254 `clamp(anisotropic, 0.0, 1.0)` guard, the #1250 `ax == ay` degeneracy, and the `0.025²` α-floor were all re-derived and verified correct this sweep (see the clean list below); they simply guard a branch nothing reaches.
- **Impact**: Two things. (a) Audit signal: the `#1250` / `#1254` regression guards are green but cover no shipping content, so "anisotropic GGX verified" overstates what is actually exercised — worth knowing before anyone spends a session re-auditing that lobe. (b) A future reader acting on the comment as written would conclude the source formats carry nothing at all and stop looking, when in fact only the magnitude is missing.
- **Suggested Fix**: Correct the comment at `material_translate.rs:575-581` to state the real situation — enable bit present in BGSM (`aniso_lighting`) and in SLSF2 for Skyrim/FO4, magnitude absent from every format, therefore deliberately not synthesised — and cross-reference `asset_provider/material.rs:1652`. Do **not** wire a fabricated strength. If the lobe is ever to be reached from content, the enable bit must be read through the `TextureSlotLayout` gate that `dedicated_shader.rs:170` already uses for the sibling SLSF2 bits, because bit 21 is `Alpha_Decal` on FO3/FNV (shader_flags.rs:412) and an ungated read would turn every FNV decal into an anisotropic surface.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D17-03

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
