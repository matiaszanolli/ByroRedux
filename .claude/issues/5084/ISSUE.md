# #5084: OBL-2026-09-29-D2-02: #4415's magic runtime and consumables look up Oblivion EFID codes by FormID; magic_effects_by_code has no consumer

**Labels**: bug, low, legacy-compat, scripting, game:oblivion, esm-plugin

**Source report**: `docs/audits/AUDIT_OBLIVION_2026-09-29.md`
**Severity**: LOW (latent: Oblivion has no AVIF records and `resolve_actor_value` has no Oblivion arm, so nothing would apply today even with the right MGEF)
**Dimension**: BSA v103 & ESM Data Slice

## Location
- `crates/scripting/src/magic.rs`: `index.magic_effects.get(&effect.effect_form_id)`.
- `crates/plugin/src/consumables.rs`: the three `magic_effects` lookups by `effect_form_id` (incl. `restoration`).
- `crates/plugin/src/esm/records/dispatch_misc_gameplay_b.rs`: builds `magic_effects_by_code` "so the (pending) magic-system runtime can resolve EFID lookups on Oblivion".
- `crates/plugin/src/esm/records/index.rs` (`magic_effects_by_code` field doc: "not yet read by any consumer").

## Description
`cd4fc019a` (#4415) landed the "pending" magic runtime, but translates spells through `magic_effects` keyed by FormID. On Oblivion the `EFID` is a 4-char effect code, so every constant-effect lookup misses and `continue`s silently; `consumables::restoration` misses the same way through `?`. This is exactly the failure mode #969 predicted ("the moment such a runtime lands … every Oblivion spell will silently no-op"); #969 added the `magic_effects_by_code` side index, and the new consumers do not use it.

## Evidence
- `Oblivion.esm` `SPEL` `EFID`s are 149 distinct codes (FOAT 136, REAT 112, DRAT 108, REHE 92, SEFF 89). As little-endian u32 values (e.g. `0x5441_4F46`) none can equal an Oblivion MGEF FormID.
- `rg magic_effects_by_code` outside `index.rs`, `dispatch_misc_gameplay_b.rs` and tests finds nothing.

Validated at HEAD 9fcfdc3fc: `magic.rs` and `consumables.rs` look up `index.magic_effects` by `effect_form_id` only; `magic_effects_by_code` has no production reader.

## Impact
When the legacy actor-value resolver lands, Oblivion abilities, birthsign and racial constant effects, and potions will still translate to nothing, with no log.

## Related
- #969 (CLOSED; built the side index and predicted this), #4415 (OPEN magic-runtime tracker), ESM-2026-09-29-D2-02 (EFID remap warnings).

## Suggested Fix
Add a single `EsmIndex` accessor that routes an effect id through `magic_effects_by_code` on Oblivion (FormID lookup elsewhere), and use it in `magic.rs` and `consumables.rs`. Pin it with an Oblivion `EFID` fixture (`b"FOAT"`).

## Completeness Checks
- [ ] **SIBLING**: Every `magic_effects.get(...effect_form_id)` site (SPEL, ENCH, ALCH, INGR) routed through the accessor
- [ ] **TESTS**: A regression test pins this specific fix

