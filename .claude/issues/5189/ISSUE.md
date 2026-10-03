# #5189 — REN-D10-2026-10-03-01: Oblivion's `__MAX_Default_Light` exporter artifact still reaches the GPU as one full-white, shadow-traced directional that lights the whole scene. Gamebryo scopes it to the ear/hair/statue subtree its root node lists it under.

**Labels**: high,renderer,nifal,game:oblivion,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: HIGH. This is a rendering-correctness problem. In every Oblivion session the first carrier mesh to load adds an unauthored, unit-white (`radiant=[1,1,1]`, `dimmer=1.0`) directional key light to every cluster. That is as bright as the authored XCLL key (`directional_color × 0.6`) or brighter. The player body's `earshuman.nif` is a carrier, so it is present from boot. #5123, also HIGH, was the 4× version of this.
- **Dimension**: Light Animation (light canonical translation)
- **Game Affected**: Oblivion only. A content scan of the meshes archive finds `NiDirectionalLight` in 48 Oblivion meshes, all named `__MAX_Default_Light`, and 0 in FNV or FO3. An `NiPointLight` sanity check on the same scanner found 38 in FNV.
- **Location**: `byroredux/src/cell_loader/spawn.rs` `spawn_nif_lights`. Also `is_known_exporter_artifact_light_name` and the dedup on `world.find_by_name`, which keeps the *first* artifact. Further down: `byroredux/src/render/lights.rs` `collect_lights` → `gpu_light_from_emitter` (`LightKind::Directional` → type `2.0`), `crates/renderer/shaders/cluster_cull.comp` (`lightType > 1.5` → "always affects all clusters"). The scope source: `crates/nif/src/import/types.rs` `ImportedLight::affected_node_names`.
- **Status**: NEW. This is the residual of #3557 and #5123, both CLOSED. Both fixed only the *multiplicity*. #3557 took "the intended synthetic contribution" as its premise, and #5123's fix (`c6dd169f8`) deliberately keeps "only the first artifact light in the process". It also regenerated the Gilded Carafe baseline to `light_count_directional 1`, so the gate now encodes the artifact as expected. #335 (CLOSED) imported the affected-node list and explicitly deferred renderer-side scoping. No issue questions whether the artifact should light the scene at all. Searched: "max_default_light", "exporter artifact light", "NiDirectionalLight", "directional light NIF", "light_count_directional".
- **Description**:
  - **Gamebryo semantics.** A dynamic effect's scope *is* its affected-node list. `gamebryo-v32/Documentation/Programmer/General_Topics/Introduction_to_Dynamic_Effects.htm` says: "Objects that are to be affected by a dynamic effect are registered with that effect using the AttachAffectedNode method. This method causes the effect to affect the entire subtree rooted at the given object."
  - **The vanilla carrier.** `meshes\characters\imperial\earshuman.nif` is NIF v10.0.1.0. Per nif.xml, that version has no on-light `Affected Nodes` list (it is absent between 4.0.0.2 and 10.1.0.0), so the scope is serialized on the node side. Byte-decoding the root `NiNode "EarsHuman"` gives `children (1, 6, 7)` and `effects (6, 7)`. Blocks 6 and 7 are the two `NiDirectionalLight`s: dimmer 1.0, ambient 0, diffuse (1,1,1), specular (1,1,1). In the legacy engine they affect the ear mesh subtree at most.
  - **What the engine does instead.** The importer leaves `affected_node_names` empty. Its doc then states the opposite of Gamebryo: "An empty `Vec` means "no restriction" (the light affects every nearby surface)". `spawn_nif_lights` ignores the list anyway. So the surviving artifact becomes a `LightSource` with `VisibilityMask::for_legacy_local_light()` (= `FULL`, so shadow-traced). `collect_lights` uploads it as a type-2.0 GpuLight in the *point-light suffix*, and `cluster_cull.comp` puts it in every cluster.
  - **Direction is arbitrary.** Its direction is whichever half of the ± key/fill pair loaded first, at identity placement on the loose/actor path. #5123's dump shows `[-0.4467, 0.7444, 0.4963]`, i.e. lit from above. It does not follow any actor.
  - **Collateral.** The `collect_lights` comment "the directional light (if present) is always exactly one entry ... everything from here on is the point-light suffix" is false whenever this light exists. The renderer's `take_while(color_type[3] > 1.5)` in `assemble_camera_and_lights.rs` can also absorb it into the "pinned" prefix when no scene key is present. Both are ordering-only side effects; there is no further correctness impact.
- **Evidence**:
  ```text
  earshuman.nif (Oblivion - Meshes.bsa), header "NetImmerse File Format, Version 10.0.1.0"
  [0] NiNode "EarsHuman"  children (1, 6, 7)  effects (6, 7)
  [6],[7] NiDirectionalLight "__MAX_Default_Light"  dimmer 1.0  diffuse (1,1,1)  spec (1,1,1)
  Carriers (48): characters\{imperial\earshuman, highelf\earshighelf, woodelf\earswoodelf, darkelf\earsdarkelf}.nif,
    14+ hair styles (style01-03, emperor, nordfemalebunches, ...), clutter\key\key.nif,
    architecture\statue\statueimperial02-05 / thesentinel / statueleyawiin01, daedric shrines, priory doors, citadel pieces, menus\*.
  ```
  ```rust
  // crates/nif/src/import/types.rs, ImportedLight::affected_node_names
  /// ... An empty `Vec` means "no restriction" (the light affects every nearby surface).
  ```
  Method: `target/debug/examples/bsa_extract_one` + `dump_nif` (prebuilt), plus a read-only Python BSA v103/v104 content scan and a byte decode of the root NiNode, all in the session scratchpad. No cargo build.
- **Impact**: Oblivion interiors and exteriors get a white key light from a fixed arbitrary direction on top of authored lighting. It casts RT shadows, and it adds a ReSTIR candidate and a GI light to every pixel. Gilded Carafe's runtime baseline now treats this as correct.
- **Related**: #3557, #5123, #335, #4395, #4972. `NiAmbientLight` was checked and dropped: the 17 Oblivion and 8 FNV carriers sampled (`weynondoor01`, `vine01`) have zero diffuse, so `is_spawnable_nif_light` already skips them.
- **Suggested Fix**: Do not spawn NIF-embedded *directional* lights as scene lights. They have no LIGH authority, and every vanilla instance is the Max artifact. At minimum, drop `is_known_exporter_artifact_light_name` matches outright instead of deduplicating them. Fix the `affected_node_names` doc: empty means "scope carried elsewhere (NiNode effects, pre-10.1) or none", never "unrestricted". Reset the Gilded Carafe `light_count_directional` baseline to 0 and confirm it with a live `light.dump` (`/audit-runtime`).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the CDB merge (`apply_cdb_material`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
