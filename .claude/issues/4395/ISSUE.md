# #4395 — NIFAL-D3-2026-09-14-01: NIF-embedded spot and directional lights get an uncited "-Z" direction that contradicts Gamebryo's (1,0,0) model direction, the sibling ESM light boundary, and the canonical per-kind sign convention

**Labels**: high,nifal,import-pipeline,renderer,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: HIGH (rendering correctness — cone/sun direction wrong on every embedded spot/directional light)
- **Dimension**: Skinning/Lights
- **Tier Violated**: no-fabrication, plus a single-boundary divergence (the NIF and ESM light boundaries disagree on one Gamebryo convention)
- **Game Affected**: every game whose NIFs embed `NiSpotLight` / `NiDirectionalLight` (Oblivion exporter `NiDirectionalLight`s, #3557, are a known population; total population not measured)
- **Location**: `crates/nif/src/import/walk/lights.rs:131-137` (`imported_light_from_base`); consumed unchanged by `byroredux/src/cell_loader/spawn.rs:1108-1125` and `byroredux/src/render/lights.rs:77-88`
- **Status**: NEW
- **Description**: `imported_light_from_base` takes every NIF light's direction as the negated *third* column of the world rotation. Its only justification is the uncited comment "Gamebryo lights point down the local -Z axis". This has two problems:
  1. **Wrong axis.** The ESM boundary `translate_light` (`byroredux/src/systems/light_anim.rs:228-239`) and `docs/engine/nifal.md` §2 Lights both explicitly reject local −Z in favour of the first column. The NIF-import boundary was never brought into line.
  2. **One vector for two opposite conventions.** The canonical `Emitter.direction` means "toward the light" for directional emitters and "outward from the source" for spots (`crates/core/src/lighting.rs:217-219`), and the shader implements both. The importer emits the same vector for both kinds, and nothing downstream negates per kind, so at least one kind has the wrong sign whichever axis is right.
- **Evidence**: The orchestrator re-read both the NIF site and the ESM site and read the headers directly:
  - `/mnt/data/src/reference/gamebryo-v32/Include/NiDirectionalLight.h:30` and `/mnt/data/src/reference/gamebryo-v32/Include/NiSpotLight.h:31-32` both say "The model direction of the light is (1,0,0). The world direction is the first column of the world rotation matrix."
  - The parser's `rows` are row-major (`crates/nif/src/stream.rs`), so the first column is `[rows[0][0], rows[1][0], rows[2][0]]`, not `-[rows[i][2]]`.
  - The −Z comment dates to the original light parse (`14e9a06b0`, #156). No test pins NIF light direction.
- **Impact**: Embedded spot cones aim along the wrong local axis. Embedded directional lights light and RT-shadow surfaces from the wrong direction. The direction is resolved once and trusted by the shader, exactly as the tier model requires, so nothing masks it.
- **Related**: #3232 (closed — added `ref_rot` rotation, kept the imported axis), #2205, #2439, #3557.
- **Suggested Fix**: Take column 0 as the emission direction, then apply the Z-up→Y-up conversion. Negate it for `LightKind::Directional` so it points toward the light per the `Emitter` contract. Decide the sign once, at this boundary. Add a fixture test for each kind with a non-identity rotation. Optionally census the embedded population (`crates/nif/examples/import_probe.rs`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
