# #4066 — ESM-2026-09-09-D3-01

`CLMT.WLST` weather FormIDs are never load-order remapped, and 100 % of the references every shipped DLC master authors are self-references — every DLC-added climate resolves to no weather

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4066 --json state`).

---

- **Severity**: HIGH
- **Dimension**: FormID & Load Order
- **Record / Sub-record**: `CLMT` / `WLST`
- **Location**:
  - `crates/plugin/src/esm/records/climate.rs:53` — `pub fn parse_clmt(form_id: u32, subs: &[SubRecord], game: GameKind) -> ClimateRecord` — **no `remap` parameter at all**
  - `crates/plugin/src/esm/records/climate.rs:82-88` — the raw read
  - `crates/plugin/src/esm/records/dispatch_misc_gameplay_a.rs:34-36` — the call site
  - Live consumers: `byroredux/src/env_translate.rs:454-467` (`resolve_default_weather`), `byroredux/src/scene/world_setup.rs:466-470`, `byroredux/src/cell_loader/exterior.rs:1657-1674`
- **Status**: NEW — same defect class as **#3400 / #3401 / #3714 / #3715** (all CLOSED). `parse_clmt` was never on the `record_parsers_with_embedded_form_ids_take_a_remap` allowlist, so no guard could see it. This is exactly the recurrence the skill's Dim-3 checklist warns is "a recurring gap, not a closed one".
- **Description**: `EsmReader::read_record_header` remaps every record's own FormID
  (`reader.rs:700`), so `EsmIndex.weathers` is keyed in **global** space
  (`dispatch_misc_gameplay_a.rs:27: index.weathers.insert(fid, parse_wthr(fid, subs, game))`).
  `parse_clmt` reads the `WLST` weather references **raw** and stores them into
  `ClimateWeather::weather_form_id`. `resolve_default_weather` then does
  `weathers.get(&best.weather_form_id)` — a raw key against a remapped map.
- **Evidence**: current code, `climate.rs:80-93`:
  ```rust
  let mut r = SubReader::new(&sub.data);
  for _ in 0..count {
      if let (Ok(fid), Ok(chance_bits)) = (r.u32(), r.u32()) {
          if fid != 0 {
              record.weathers.push(ClimateWeather {
                  weather_form_id: fid,          // <-- raw plugin-local id
                  chance: chance_bits as i32,
              });
  ```
  and `byroredux/src/env_translate.rs:459-466`:
  ```rust
  let best = climate.weathers.iter().filter(|w| w.chance >= 0).max_by_key(|w| w.chance)?;
  weathers.get(&best.weather_form_id).map(|wthr| (wthr, best.chance))
  ```
  The sibling `WATR` arm three lines below the `CLMT` arm in the same dispatcher
  (`dispatch_misc_gameplay_a.rs:44-46`) *does* call `reader.get_form_id_remap()` for its
  `XNAM` (#3200) — so the pattern is established, `CLMT` was simply missed.

  **Census over every shipped DLC master that authors a `CLMT`** (bounded read-only
  walker, `/tmp/audit/esm/clmt.py`; `self-ref` = top byte equals the plugin's master
  count, the exact non-identity case `allocate_global_slot` produces):

  | plugin | masters | CLMT | WLST refs | self-ref | non-zero mod-index |
  |---|---|---|---|---|---|
  | Anchorage.esm | 1 | 3 | 3 | **3** | 3 |
  | BrokenSteel.esm | 1 | 2 | 2 | **2** | 2 |
  | PointLookout.esm | 1 | 1 | 1 | **1** | 1 |
  | ThePitt.esm | 1 | 5 | 5 | **5** | 5 |
  | DeadMoney.esm | 1 | 5 | 5 | **5** | 5 |
  | HonestHearts.esm | 1 | 4 | 5 | **5** | 5 |
  | OldWorldBlues.esm | 1 | 1 | 1 | **1** | 1 |
  | LonesomeRoad.esm | 1 | 5 | 5 | **5** | 5 |
  | Dawnguard.esm | 2 | 1 | 2 | **2** | 2 |
  | Dragonborn.esm | 2 | 2 | 2 | **2** | 2 |
  | DLCCoast.esm | 1 | 3 | 7 | **7** | 7 |
  | DLCNukaWorld.esm | 1 | 1 | 1 | **1** | 1 |

  **42 of 42** — every single weather reference authored by a DLC climate points at a
  weather the same DLC adds. There is no case where the base game's identity remap
  masks the bug.
- **Impact**: on any real multi-master load order —
  `--master Fallout3.esm --esm PointLookout.esm`,
  `--master Skyrim.esm --master Update.esm --esm Dragonborn.esm`,
  `--master Fallout4.esm --esm DLCCoast.esm`, and **every ESL by construction** — a DLC
  worldspace's climate resolves `resolve_default_weather` to `None`. `world_setup.rs:470`
  then takes the "no climate / no default weather" branch and the worldspace falls back
  to the **procedural sky** (`env_translate.rs:1338`), losing the authored fog, cloud
  layers, sun glare and TOD breakpoints for Point Lookout, Far Harbor, Solstheim, the
  Sierra Madre and the Big MT. Vanilla single-plugin loads are unaffected (mod_index 0 →
  slot 0 is identity), which is why this has never shown up in a `--esm <master>` run.
- **Related**: #3400, #3401, #3714, #3715, #3200; D3-02 below; `/audit-esm` 2026-08-30 D3-01.
- **Suggested Fix**: give `parse_clmt` a `remap: &Option<FormIdRemap>` parameter, wrap
  the `WLST` read in `remap_fid`, thread `reader.get_form_id_remap()` at
  `dispatch_misc_gameplay_a.rs:34` the way the `WATR` arm below it already does, and add
  `parse_clmt` to the allowlist in `records/tests.rs:2245`.

---
