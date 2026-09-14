# #4402 — NIFAL-D8-2026-09-14-03: #4286 inverted #2108's "a BGSM that wins the greyscale slot is authoritative, including OFF" rule, but the contract comment and test doc still state it

**Labels**: low,nifal,import-pipeline,bug,game:fo4
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (tiny FO4 population: 11 of 30,166 lit properties have slot 3 empty)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: no-fabrication (precedence policy changed with no source for which side the engine honours)
- **Game Affected**: FO4 (FO76/Starfield via the CRC-array half)
- **Location**: `byroredux/src/asset_provider/material/merge.rs:656-657` (contract bullet), `:668-684` (code now ORs), `byroredux/src/asset_provider/tests/bgsm_merge.rs:2187-2219` (doc)
- **Status**: NEW (introduced by `d28722fbf`, the fix for closed #4286)
- **Description**: All three merge branches now leave a NIF-set palette bit on, so a BGSM that wins the slot and authors the remap OFF no longer turns it off. The bullet directly above the code still says "(assignment, unchanged)". #4286's justification cited Skyrim-layout BGSM, which barely applies (BGSM is FO4+). The reachable FO4 case was not analysed. Both halves of the #3897/#3898 two-gate invariant still hold; nothing is dropped.
- **Evidence**: `byroredux/src/asset_provider/material/merge.rs:683` `bgsm_greyscale_lut_enabled |= …` sits under the bullet that says "assignment". The old `bgsm_winning_the_slot_still_authors_the_enable_bit_off` test still passes only because its NIF bit is false.
- **Impact**: FO4 content whose BGSM deliberately disables the remap while the NIF enables it renders the palette branch anyway. The larger cost is a self-contradicting precedence contract in the file that owns it.
- **Related**: #2108, #3897, #3898, #4286.
- **Suggested Fix**: Decide the rule from a source (does an FO4 named material file replace the NIF's SLSF1 bits?). Then either restore assignment in the `is_none()` branch or keep OR, and update the `:656-657` bullet and the `byroredux/src/asset_provider/tests/bgsm_merge.rs:2187` doc to match.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
