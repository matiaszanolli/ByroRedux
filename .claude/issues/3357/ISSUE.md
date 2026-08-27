# SKY-2026-08-27-D3-02: `resolve_armor_mesh` returns one ARMA mesh for a multi-part ARMO — the race skin resolves to a feet NIF, so 2,068 of 5,118 Skyrim NPCs render with no torso or hands

Labels: high,esm-plugin,bug,game:skyrim,legacy-compat

- **Severity**: HIGH
- **Confidence**: CONFIRMED (code read + real `Skyrim.esm` + archive presence check)
- **Location**: `crates/plugin/src/equip.rs:122` (signature), `:165` (pass 1 `return`), `:179` (pass 2 `return`); consumers `byroredux/src/npc_spawn.rs:797` (race skin) and `:911` (OTFT/CNTO items)
- **Description**:
  On Skyrim+ an `ARMO` links N `ARMA` armour addons; the ARMO's `BOD2` mask is the
  **union** of the regions those addons cover, and each addon supplies its own NIF.
  `resolve_armor_mesh` returns `Option<&'a str>` and `return`s from inside its
  race-match loop on the first hit:

  ```rust
  for &arma_fid in armatures {
      ...
      if race_match {
          if let Some(path) = pick_path(arma) { return Some(path); }   // :157
      }
  }
  ```

  For a single-region item (a cuirass, a ring) that is right. For the race default
  skin — the layer #2093 added precisely to cover the regions gear leaves bare — it is
  wrong: `SkinNaked` (`0x00000D64`, `BOD2 = 0x0000008D` = Head|Body|Hands|Feet) carries
  **25 ARMAs**, and for any given race three of them match (`NakedTorso*`,
  `NakedHands*`, `NakedFeet*`). Only the first in list order is returned.

- **Evidence**:
  `SkinNaked` armature table (excerpt) — the generic human family, matched via
  `additional_races` for Breton `0x13741`, Nord `0x13746`, Redguard `0x13748`, …:

  ```
  SKIN 00000D64 edid=SkinNaked bod2=0x0000008D armatures=25
    ...
    ARMA 00000D6E NakedFeet   race=00000019 addl=[13741,13744,13746,13747,13748,...]
       M='Actors\Character\Character Assets\MaleFeet_1.nif'
    ARMA 00000D6C NakedHands  race=00000019 addl=[ same ]
       M='Actors\Character\Character Assets\MaleHands_1.nif'
    ARMA 00000D67 NakedTorso  race=00000019 addl=[ same ]
       M='Actors\Character\Character Assets\MaleBody_1.NIF'
  ```

  `NakedFeet` precedes `NakedHands` and `NakedTorso` in the armature list, so the loop
  returns the feet mesh. Production `build_npc_equip_state` over real `Skyrim.esm`,
  all six Bannered Mare NPCs — every skin row resolves to a *Feet* NIF:

  ```
  == Saadia   fid=00013BA2 race=00013748(RedguardRace) skin=00000D64 gender=Female
     MESH fid=00000D64 hidden=0x00000004 path=Actors\Character\Character Assets\FemaleFeet_1.nif
  == Brenuin  fid=00013BA7 race=00013748  MESH 00000D64 hidden=0x00000004 ...\MaleFeet_1.nif
  == Mikael   fid=0001A670 race=00013746  MESH 00000D64 hidden=0x00000080 ...\MaleFeet_1.nif
  == Sinmir   fid=000813B5 race=00013746  MESH 00000D64 hidden=0x00000004 ...\MaleFeet_1.nif
  == AmaundMotierreEnd fid=0004E64F race=00013741 MESH 00000D64 hidden=0x00000080 ...\MaleFeet_1.nif
  == Hulda    fid=00013BA3 race=00013746  MESH 00000D64 hidden=0x00000080 ...\FemaleFeet_1.nif
  ```

  Population sweep over all 5,118 `Skyrim.esm` NPCs, using each NPC's own
  `RACE.WNAM` skin and the production equip builder:

  ```
  NPCs whose race skin still owns biped bit 2 (torso): 3445
    ...of which the queued skin mesh is a FEET/HANDS nif (or none): 2068
    {"...\FemaleFeet_1.nif": 617, "...\MaleFeet_1.nif": 1451}
  ```

  The `hidden_biped_mask` half of #2094 then misfires on the wrong NIF: Mikael, Hulda
  and Amaund get `hidden = 0x80` (feet displaced by boots) applied to a **feet** mesh,
  so `hide_skin_partitions` strips the whole thing and the skin contributes nothing at
  all; Saadia, Brenuin and Sinmir get `hidden = 0x04` (torso) applied to a feet mesh,
  which is a no-op, so their feet render but their hands never do.

  The correct meshes are present in the archives, so this is a resolver defect, not a
  missing asset:

  ```
  FOUND    meshes\actors\character\character assets\malebody_1.nif
  FOUND    meshes\actors\character\character assets\femalebody_1.nif
  FOUND    meshes\actors\character\character assets\malehands_1.nif
  FOUND    meshes\actors\character\character assets\malefeet_1.nif
  ```

  `166` of 2,762 Skyrim ARMO records have more than one ARMA serving the same race, so
  the single-return shape is wrong beyond the skin as well. No other consumer reads
  `ItemKind::Armor::armatures` — `resolve_armor_mesh` is the only path from ARMO to a
  worn mesh (`grep -rn "armatures"` over the tree).

- **Impact**:
  2,068 of 5,118 vanilla Skyrim NPCs (40%) render with bare feet where their torso
  should be and no hands. On the `WhiterunBanneredMare` bench cell all six named NPCs
  are hit; Hulda, Mikael and Amaund Motierre reduce to a FaceGen head plus jewellery
  plus boots with no body between them. #2093's stated purpose — "an NPC whose
  OTFT/CNTO doesn't cover a biped region has zero mesh source there" — is not achieved
  for the two regions (torso, hands) it most matters for.

- **Suggested Fix**:
  Change the signature to yield every matching addon rather than the first:
  `pub fn resolve_armor_meshes<'a>(...) -> Vec<&'a str>` — pass 1 collects **all**
  race-matching ARMAs with a non-empty gender mesh (dedup by path; several ARMAs can
  share one NIF), pass 2 keeps today's single "first non-empty" fallback for the
  no-race-match case. `build_npc_equip_state` pushes one `ResolvedArmor` per returned
  path, all sharing the same `inv_idx` so the #2094 `retain` and the
  `hidden_biped_mask` assignment (which already iterates `armor_to_spawn` with `find`
  — switch to `filter`) keep working unchanged. Keep a thin
  `resolve_armor_mesh` wrapper returning `.first()` if any caller wants one path.

- **Related**: #2093, #2094 (both CLOSED — the mechanisms they added are present and
  correct at HEAD; this is the layer below them). No OPEN issue covers it.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
