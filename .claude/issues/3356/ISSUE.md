# SKY-2026-08-27-D3-01: `parse_otft` reads only the first FormID of the `INAM` array — 765 of 1,246 Skyrim outfit items (61%) are silently dropped

Labels: high,esm-plugin,bug,game:skyrim,game:fo4,legacy-compat

- **Severity**: HIGH
- **Confidence**: CONFIRMED (code read + raw-byte walk of `Skyrim.esm` + live equip-chain trace)
- **Location**: `crates/plugin/src/esm/records/outfit.rs:73`
- **Description**:
  Skyrim+ `OTFT` carries its item list as **one `INAM` sub-record containing an array
  of 4-byte FormIDs** (xEdit `wbDefinitionsTES5.pas`: `wbArray(INAM, 'Items',
  wbFormIDCk('Item', [ARMO, LVLI]))`). The parser instead treats each `INAM`
  sub-record as a *single* FormID:

  ```rust
  b"INAM" if sub.data.len() >= 4 => {
      if let Ok(id) = SubReader::new(&sub.data).u32() {
          out.items.push(remap_fid(id, remap));
      }
  }
  ```

  `SubReader::u32()` consumes the first 4 bytes and the arm returns; bytes 4..N of the
  array are never read. Every outfit therefore yields exactly one item regardless of
  how many it authors. The length guard (`>= 4`) makes the truncation invisible — a
  20-byte 5-item `INAM` passes it and contributes one item.

- **Evidence**:
  Raw walk of the `OTFT` top-level GRUP in `Skyrim.esm` (record header 24 B,
  sub-record header 6 B, zlib-decompressing flagged records), counting `INAM`
  sub-record payload sizes:

  ```
  Skyrim.esm     OTFT=481 total item FormIDs=1246 parsed=481 DROPPED=765
                 INAM byte-size histogram {4: 94, 8: 144, 12: 131, 16: 91, 20: 19, 24: 2}
  Dawnguard.esm  OTFT=69  total item FormIDs=191  parsed=69  DROPPED=122
  HearthFires.esm OTFT=3  total item FormIDs=8    parsed=3   DROPPED=5
  Dragonborn.esm OTFT=57  total item FormIDs=161  parsed=57  DROPPED=104
  Fallout4.esm   OTFT=388 total item FormIDs=720  parsed=388 DROPPED=332
  ```

  Every one of the 481 Skyrim outfits has exactly **one** `INAM` sub-record; 387 of
  them are longer than 4 bytes. The five outfits worn by the six Bannered Mare NPCs,
  raw (full array vs. what `parse_otft` keeps — only the first entry):

  ```
  0002D75E FarmClothesOutfit02      ['000209A5', '000209A6']
  0005FB81 BarkeepClothes01         ['0005B6A1', '0005B6A0']
  00028B61 BeggarWithHatOutfit      ['00013105', '00013106', '00013104']
  000B1FAE ArmorBandedIronAllOutfit ['00013948', '00012E46', '00012E4B', '00012E4D']
  000E40DD FineClothesOutfit02      ['000CEE82', '000CEE80']
  ```

  Driving the production `build_npc_equip_state` over real `Skyrim.esm` confirms the
  loss reaches the equip state — `FarmClothesOutfit02`'s second entry (`000209A6`,
  the farm-clothes torso) is gone, so Hulda and Mikael end up with the race skin
  occupying the torso slot instead of clothing:

  ```
  Mikael:  b0=00000D64/SkinNaked  b2=00000D64/SkinNaked  b3=00000D64/SkinNaked
           b5=000877DC  b6=0001CF2B  b7=000209A5/ClothesFarmBoots02
  Hulda:   b0=00000D64/SkinNaked  b2=00000D64/SkinNaked  b3=00000D64/SkinNaked
           b5=00087832  b6=000877A7  b7=000209A5/ClothesFarmBoots02
  Sinmir:  b0=00000D64/SkinNaked  b2=00013948/ArmorIronBandedCuirass
           b3=00000D64/SkinNaked  b7=00000D64/SkinNaked      (no boots/gauntlets/helmet)
  ```

  The one fixture that should have caught this models a wire shape the game never
  emits — four separate 4-byte `INAM` sub-records, under a comment asserting it is
  real (`crates/plugin/src/esm/records/outfit.rs:105-123`):

  ```rust
  // Models a real OTFT shape: `WhiterunGuardOutfit` with helmet,
  // cuirass, gauntlets, boots referenced by INAM.
  let subs = vec![ edid("WhiterunGuardOutfit"),
      inam(0x0001_3937), inam(0x0001_3938), inam(0x0001_3939), inam(0x0001_393A) ];
  ```

  There is no `0x0008F09E` and no `WhiterunGuardOutfit` in `Skyrim.esm`. The four real
  Whiterun guard outfits each carry a single `INAM`:

  ```
  000D33C6 GuardWhiterunOutfit                 [('EDID',20),('INAM',4)]  ['000D33C7']
  00104F3F GuardWhiterunOutfitNoHelmetNoShield [('EDID',36),('INAM',8)]  ['0002150D','000A6D7F']
  0010B2EF GuardWhiterunOutfitNormalHelmet     [('EDID',32),('INAM',4)]  ['0010B2F0']
  000DD052 GuardWhiterunOutfitNoHelmet         [('EDID',28),('INAM',4)]  ['000E962D']
  ```

- **Impact**:
  Every Skyrim/FO4 NPC whose outfit authors more than one item spawns partially
  clothed. 387 of 481 Skyrim outfits (80%) are truncated; on the reference
  `WhiterunBanneredMare` bench cell that means Hulda and Mikael lose their torso
  clothing entirely and Sinmir loses boots, gauntlets and helmet from a 4-piece armour
  set. Compounds with SKY-…-D3-02 below: the lost torso item is exactly what would
  have displaced the (wrongly-resolved) race skin. Also drops 231 items across the
  three Skyrim DLC masters and 332 across `Fallout4.esm`.

- **Suggested Fix**:
  Iterate the `INAM` payload in 4-byte strides instead of reading one `u32`:
  ```rust
  b"INAM" => {
      for chunk in sub.data.chunks_exact(4) {
          let id = u32::from_le_bytes(chunk.try_into().expect("chunks_exact(4)"));
          out.items.push(remap_fid(id, remap));
      }
  }
  ```
  `chunks_exact` preserves the existing "short/trailing bytes are dropped" contract
  that `malformed_inam_short_payload_is_dropped` pins. Replace the fabricated
  `parses_outfit_with_multiple_items` fixture with the real shape — a single 16-byte
  `INAM` — and add a real-data assertion that `ArmorBandedIronAllOutfit` (`000B1FAE`)
  parses to 4 items.

- **Related**: no OPEN issue covers OTFT/`INAM`. #2079/#1996 touched this function
  (FormID remap) without noticing the array shape. Distinct from #3217 (LVLI flags).

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
