# #969 — Investigation

## Gate check (issue says "defer until consumer materializes")

- `parse_mgef`, `parse_spel`, `parse_ench` all live and populate
  `EsmIndex.magic_effects / spells / enchantments`
  ([crates/plugin/src/esm/records/misc/magic.rs](../../crates/plugin/src/esm/records/misc/magic.rs)).
- ALCH and INGR DO already read `EFID` and stash it as
  `Vec<u32>` ([items.rs:514-516](../../crates/plugin/src/esm/records/items.rs#L514-L516),
  [items.rs:533-538](../../crates/plugin/src/esm/records/items.rs#L533-L538)).
  On Oblivion those u32s are the 4-byte code reinterpreted, not a
  FormID — so they already represent latent silent-no-op data on disk
  the moment a consumer reads them.
- No runtime spell-casting / enchant-resolution consumer exists yet in
  `byroredux/`, `crates/scripting/`, or `crates/papyrus/` (grepped).

The "gate" condition (EFID is actually being parsed) is met by
ALCH/INGR; the consumer is still pending. Implementing the secondary
map now is cheap and removes the foot-gun before the consumer lands.
The map is additive (no behavior change for non-Oblivion content) and
gated on `GameKind::Oblivion` so it can't shadow FNV/Skyrim entries
that happen to have 4-char EDID prefixes.

## Implementation plan

1. Add `pub magic_effects_by_code: HashMap<[u8; 4], u32>` field to
   `EsmIndex` in `crates/plugin/src/esm/records/index.rs`.
2. Populate at the MGEF extraction site in
   `crates/plugin/src/esm/records/mod.rs` (line 496-498) when
   `game == GameKind::Oblivion` AND `editor_id.len() == 4` (Oblivion
   MGEFs use the fixed `[u8; 4]` code as EDID; `read_zstring` already
   strips the trailing null so len == 4 is exactly the Oblivion shape).
3. Add `magic_effects_by_code` to the `categories()` table for
   regression visibility in the end-of-parse summary line.
4. Add `magic_effects_by_code` to `merge_from()` for multi-plugin DLC
   support (last-write-wins, matching `magic_effects` itself).
5. Tests in `records/misc/magic.rs` or a sibling — assert:
   - Oblivion 4-char EDIDs populate the map keyed on the bytes.
   - Non-Oblivion games leave the map empty.
   - Oblivion EDIDs that aren't 4 chars (defensive) don't crash and
     don't populate.

## Sibling check

- FO3/FNV/Skyrim path unchanged — the FormID-keyed `magic_effects`
  map is untouched. Only the gated secondary map differs.
- ALCH/INGR EFID parsing already reads as `u32`. On Oblivion the u32
  IS the 4-byte code (little-endian); consumer when it lands will
  `to_le_bytes()` back to `[u8; 4]` and look up via the new map.
  Not changing ALCH/INGR right now — out of scope for this issue.

## Scope

3 files modified: `records/index.rs`, `records/mod.rs`, plus tests
(co-located in `records/misc/magic.rs`). Under the 5-file threshold.
