# SKY-2026-08-27-D3-03: no guard pins the Whiterun Bannered Mare NPCs, and no test anywhere walks the Skyrim equip chain on real ESM data

Labels: medium,esm-plugin,test-gap,bug,game:skyrim,legacy-compat

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (exhaustive grep + full `--ignored` inventory)
- **Location**: `byroredux/src/npc_spawn/tests.rs` (whole file), `crates/plugin/tests/parse_real_esm.rs`
- **Description**:
  Checklist item 1 asks for the guard that pins the six named Bannered Mare NPCs to
  Inventory + EquipmentSlots + OTFT/LVLI-dispatched equip. **It does not exist.**
  Nothing in the tree names `saadia`, `brenuin`, `mikael`, `sinmir`, `hulda`, or
  `amaundmotierre` outside a path-formatting assertion, and no test drives
  `build_npc_equip_state` against a real ESM on any game.
- **Evidence**:
  ```
  $ grep -rn "saadia\|brenuin\|mikael\|sinmir\|hulda\|amaundmotierre" --include="*.rs" .
  byroredux/src/npc_spawn/tests.rs:169:  // Vanilla SSE Whiterun Mikael (FormID 0x00013BBE in
  byroredux/src/npc_spawn/tests.rs:172:  prebaked_facegen_nif_path("Skyrim.esm", 0x00013BBE),
  byroredux/src/npc_spawn/tests.rs:187:  prebaked_facegen_tint_path("Skyrim.esm", 0x00013BBE),
  ```
  That is `prebaked_facegen_nif_path_matches_vanilla_layout` — a pure string-format
  test on a hard-coded FormID; it never opens an ESM and asserts nothing about equip.
  (It also cites `0x00013BBE` as "Mikael" while `Skyrim.esm`'s Mikael is `0x0001A670`;
  `0x00013BBE` is some other record. Cosmetic, but the comment is wrong.)

  All eleven `build_npc_equip_state` call sites in `npc_spawn/tests.rs` construct
  synthetic `EsmIndex` fixtures (`:505 :550 :592 :782 :864 :964 :1037` …). The three
  that cover #2093/#2094 —
  `prebaked_equip_state_falls_back_to_race_skin_for_uncovered_slots`,
  `prebaked_equip_state_marks_only_partially_displaced_skin_slots`,
  `prebaked_equip_state_drops_skin_mesh_fully_displaced_by_gear` — each give the skin
  ARMO exactly **one** ARMA, which is why SKY-…-D3-02 is invisible to them. The only
  real-data NPC test in the crate is
  `npc_spawn/ai_package.rs::real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs`,
  which covers PKID, not equip. The 21 `#[ignore]` tests in
  `crates/plugin/tests/parse_real_esm.rs` cover parse rates, GLOB/AVIF/CLAS/RACE/WATR
  and one FNV LVLI pin — none touch OTFT, ARMA, or the equip chain.

- **Impact**:
  Two HIGH-severity defects that a ~2 s real-data test makes obvious (the throwaway
  used for this audit parses `Skyrim.esm` in 1.8 s) shipped and survived a prior D3
  audit pass. Both are on the bench-of-record cell.
- **Suggested Fix**:
  Add one `#[ignore]` real-data test beside the existing `real_skyrim_esm_ambient_*`
  guard, using the same `BYROREDUX_SKYRIM_DATA`-with-default + self-skip convention:
  resolve the six NPCs by `editor_id`, assert all six are found, and for each assert
  (a) `Inventory` is non-empty, (b) `EquipmentSlots` occupies the expected biped bits,
  (c) the queued mesh set covers biped bit 2 with a torso NIF and bit 3 with a hands
  NIF, and (d) `ArmorBandedIronAllOutfit` contributes 4 items to Sinmir. (a) and (b)
  pass today; (c) and (d) are the ones that fail and would have caught both defects.
- **Related**: none open.

---

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
