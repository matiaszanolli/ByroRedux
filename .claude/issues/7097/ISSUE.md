# #7097 — ESM-02: Oblivion `ALCH` effect chain fed `&None`: `SCIT` script FormID skips the load-order remap

Labels: medium,esm-plugin,scripting,bug,game:oblivion
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-02)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-02**

- **Severity**: MEDIUM (a live consumer runs the unremapped id; vanilla masked)
- **Dimension**: FormID Remap (shared decoder called with an identity remap). Found independently as D3-01 and D4-07.
- **Record / Sub-record**: Oblivion `ALCH` / `SCIT` (script FormID @0)
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:896-904` (`oblivion_effect_chain` → `chain.feed(sub, &None)`). It is reached from `crates/plugin/src/esm/records/items/consumable.rs:29-46`, whose Oblivion early return drops the `remap` it holds, and consumed at `crates/scripting/src/magic_effects.rs:99-102` (`cast_effect`, `item.script_form_id`).
- **Status**: NEW (introduced by `9cb2703b3`, Refs #7045)
- **Description**: xEdit TES4 `wbEffects` (`wbDefinitionsTES4.pas:1492-1517`) defines `SCIT` as `wbFormIDCk('Script effect', [SCPT, NULL])` followed by school, visual char4 and hostile byte. The shared `MagicEffectAccumulator::feed` remaps `SCIT` @0, and SPEL/ENCH pass their remap. The new ALCH route builds its own accumulator and feeds it `&None`. `EFID` (char4) and `EFIT` stay correctly unremapped, so the one FormID this helper decodes is the one it skips. `no_parser_with_a_remap_in_scope_passes_an_identity_remap` scans only `parse_*` bodies, so it cannot see this.
- **Evidence**:
  - `Oblivion.esm` has 253 ALCH, 9 of them with a `SCIT`, all at master index 0, so the identity remap is harmless on vanilla.
  - The DLCs author `SCIT` only on INGR/ENCH/SPEL, which do remap.
- **Impact**: A mod's potion whose script effect names its own (or a non-slot-0 master's) script runs the wrong script, or none, on a multi-plugin Oblivion load order.
- **Suggested Fix**: Give `oblivion_effect_chain` a `remap` parameter and pass `parse_effects`' remap through it. Add a non-identity-remap Oblivion ALCH `SCIT` test modelled on `parse_spel_efid_is_remapped`. Consider widening the identity-remap guard to non-`parse_*` helpers that call `feed`/`push_ctda`.

Independent reports: ESM-D3-01, ESM-D4-07 (appendix of the report).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
