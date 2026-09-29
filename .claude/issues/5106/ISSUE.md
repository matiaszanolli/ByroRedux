# NIFAL-D8-2026-09-29-01: #4636's dead-path yield lets a BGSM win the greyscale-LUT texture while the #4402 enable-bit rule still treats the slot as NIF-won

**Labels**: low,bug,nifal,import-pipeline,game:fo4,game:fo76

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW (latent; population not censused)
**Dimension**: Shader flags / texture roles (merge boundary)
**Tier Violated**: single-boundary (two precedence decisions for one slot disagree) · **Game Affected**: FO4, FO76
**Location**: `byroredux/src/asset_provider/material/merge.rs`:
- `nif_supplied_greyscale_lut` captured before the chain walk;
- the bit rule: if the slot is empty, assign (BGSM wins); else if the NIF supplied the slot, OR;
- `fill(&mut material.textures.greyscale_lut, &bgsm.greyscale_texture, …, texture_exists)` runs **after** the bit rule;
- the #4636 replacement branch of `fill` (dead NIF path + live sidecar path → replace).

## Description
Suppose the NIF's slot-3 LUT path resolves in no archive and the BGSM's does. The bit block sees a filled slot, takes the NIF-won branch, and ORs the BGSM bit in. `fill` then replaces the slot with the BGSM's LUT. The material ends up sampling the BGSM's texture under an enable that is NIF SLSF1 bit OR BGSM bit. That violates the #2108/#4402 contract documented in the same function ("this BGSM wins the slot → authoritative for both the texture and the enable bit (assignment, including OFF)"). Because `nif_supplied_greyscale_lut` stays true, every ancestor step in the chain also ORs. Introduced by the interaction with `78d079707` (#4636).

## Evidence
The outcome diverges only when the NIF bit is ON, the BGSM bit is OFF, the NIF LUT is dead and the BGSM LUT is live — then the remap stays on although the winning material authored it off. #4636's census covered the normal slot only (3 dead FO4 paths out of 637 disagreements), so vanilla incidence for the LUT slot is unmeasured (likely ~0); mods/retextures can reach it. The BGEM arm assigns the LUT without `fill`, so #4636 never applies there.

## Impact
Wrong palette remap on the affected mesh. Below the HIGH `translate_material` floor only because there is no known population.

## Related
#4636, #4402, #3898, #2108, #4286.

## Suggested Fix
Decide the winner once: run the LUT `fill` first, have it report whether this BGSM's path took the slot, and apply assign-vs-OR from that result.

Validated at HEAD 9fcfdc3fc: in `merge.rs` the `if material.textures.greyscale_lut.is_none() { assign } else if nif_supplied_greyscale_lut { OR }` block precedes `fill(&mut material.textures.greyscale_lut, …)`, and `fill` replaces a dead NIF path with a live sidecar path.

## Completeness Checks
- [ ] **SIBLING**: other slots whose enable bits are decided before `fill` (and the BGEM arm)
- [ ] **CANONICAL-BOUNDARY**: fix stays in the merge / `translate_material` boundary, never in shaders
- [ ] **TESTS**: A regression test pins the dead-NIF-LUT + live-BGSM-LUT + BGSM-bit-OFF case
