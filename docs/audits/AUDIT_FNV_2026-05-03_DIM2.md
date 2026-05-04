# FNV ESM Record Parser Audit — Coverage & Accuracy — 2026-05-03

**Auditor**: Claude Opus 4.7 (1M context)
**Scope**: `--focus 2` (Dim 2 — ESM record parser coverage & accuracy). FNV is the engine's reference title; Dim 2 audits whether dispatch coverage matches what's on disk.
**Reference reports**:
- `docs/audits/AUDIT_FNV_2026-04-24.md` — last broad FNV audit (4 prior FNV-D2 findings, all closed since)
- `docs/audits/AUDIT_M33_2026-04-21.md` — sky/weather sub-record decode audit
**Open-issue baseline**: 51 OPEN at audit start; 0 with `FNV-D2` prefix or `FNV-ESM` (all prior findings closed).

---

## Executive Summary

**0 CRITICAL · 1 HIGH · 1 MEDIUM · 1 LOW** — across 3 new findings.

The headline number: **FalloutNV.esm has 101 distinct top-level GRUP record types; the parser dispatches 58 of them. 43 record types still drop at the catch-all `_ => skip_group()` arm.** Of those 43, ~5 are HIGH-impact gameplay-relevant (PROJ / IMOD / EFSH / ARMA / BPTD), ~7 are MEDIUM, and the remaining ~31 are LOW-priority supporting records (audio metadata, load screens, FNV hardcore mode, Caravan / Casino content).

The four prior FNV-D2 findings (#629 ENCH, #630 FLST, #631 INFO walk, #634 EsmIndex.total) are all CLOSED. Categorical baseline holds:

| Metric | 04-24 baseline | 05-03 measured | Delta |
|---|---:|---:|---|
| Total records parsed | ~13 684 (M24 Phase 1) | **64 446** | +50 762 (PACK/QUST/DIAL/MESG/PERK/SPEL/MGEF/ENCH/AVIF/ACTI/TERM/FLST landed) |
| items | 2643 | 2643 | ✓ |
| containers | 2478 | 2478 | ✓ |
| LVLI | 2738 | 2738 | ✓ |
| LVLN | 365 | 365 | ✓ |
| NPCs | 3816 | 3816 | ✓ |
| factions | 682 | 682 | ✓ |
| globals | 218 | 218 | ✓ |
| game_settings | 648 | 648 | ✓ |
| packages | — | 4163 | NEW |
| quests | — | 436 | NEW |
| dialogues | — | 18215 | NEW (DIAL+INFO via #631) |
| messages | — | 1144 | NEW |
| perks | — | 176 | NEW |
| spells | — | 270 | NEW |
| magic_effects | — | 289 | NEW |
| activators | — | 1143 | NEW |
| terminals | — | 344 | NEW |
| form_lists | — | 464 | NEW (FLST via #630) |

Wall time: **1.09 s** for full FalloutNV.esm parse to typed records. Holds.

| Sev | Count | NEW IDs |
|--|--:|--|
| CRITICAL | 0 | — |
| HIGH | 1 | FNV-D2-NEW-01 (5 record-type bundle: PROJ / IMOD / EFSH / ARMA / BPTD) |
| MEDIUM | 1 | FNV-D2-NEW-02 (7 record-type bundle: COBJ / IDLE / CSTY / IPCT / IPDS / EXPL / REPU) |
| LOW | 1 | FNV-D2-NEW-03 (31 record-type long tail) |

### What's confirmed CLOSED since 2026-04-24

| Issue | Status |
|---|---|
| #629 (FNV-D2-01 ENCH) | Closed — `parse_ench` at `mod.rs:752`; 270 records on FNV.esm |
| #630 (FNV-D2-02 FLST) | Closed — `parse_flst` at `mod.rs:796`; 464 records |
| #631 (FNV-D2-03 INFO walk) | Closed — `extract_dial_with_info` at `mod.rs:739`; 18215 dialogues |
| #634 (FNV-D2-06 EsmIndex.total) | Closed — single source of truth via `categories()` table |
| #519 (AVIF dispatch) | Closed — `parse_avif` at `mod.rs:762` |
| #520 (PerkRecord stub) | Closed — `parse_perk` at `mod.rs:744` |
| #521 (ACTI/TERM as MODL statics only) | Closed — dual-target via `extract_records_with_modl` at `mod.rs:771-788` |
| #527 (two-pass walk regression) | Closed — `extract_records_with_modl` fuses |

### What's verified WORKING (no findings)

- **CELL XCLL `fog_far_color` optional handling** (`cell/walkers.rs:174-204`): correctly gated on `sub.data.len() >= 92`. FNV's XCLL is at most 40 bytes (28 prefix + 12 FNV extension at `>= 40`), so `fog_far_color` stays `None` for FNV. Skyrim+ XCLL hits the `>= 92` arm. Dual-gate matches the on-disk shape per #379.
- **`unreachable_patterns` warning at the historical `cell.rs:211` site**: closed via #378 on 2026-04-20. Build is warning-free at HEAD.
- **FO4 SCOL/MOVS/PKIN/TXST cross-contamination**: dispatched at top level of `parse_esm_with_load_order` (lines 487-490), NOT inside the STAT-like multi-match block at `mod.rs:495-496`. No conflict with FNV STAT-like records. SCOL appears in FNV.esm but the FO4 28-byte placement layout fits FNV's vanilla SCOL (verified by 04-24 audit's #527 closeout).
- **Spot-check Varmint Rifle (00004337)**: weapon parses with full WEAP fields; ammo/skill bonuses route through AVIF correctly.
- **Spot-check NCR faction (000F43DD)**: FACT data byte (post-#481/#482/#483) + XNAM combat reactions correct.
- **Spot-check VATS AVIF (any 0001A0xx)**: AVIF dispatched and indexed via `parse_avif`; consumers (NPC skill bonuses, BOOK skill-book teach refs) resolve.

---

## Coverage Gap — FalloutNV.esm Top-Level GRUPs Not Dispatched

The single architectural finding: 43 of FalloutNV.esm's 101 top-level GRUP record types fall through the `_ => skip_group()` catch-all at `crates/plugin/src/esm/records/mod.rs:799-801`. They're tiered below by impact on FNV gameplay / rendering correctness.

The pattern has been the same in every prior FNV-D2 audit: each round closes ~3-4 high-priority types (#629 ENCH, #630 FLST, #631 INFO, etc.). This audit identifies the next tier.

---

## Findings

### HIGH

#### FNV-D2-NEW-01 — 5 gameplay-critical record types still drop at catch-all skip

- **Severity**: HIGH (5-record bundle; each individually MEDIUM, bundled HIGH because of cumulative gameplay coverage)
- **Dimension**: ESM Record Parser
- **Location**: `crates/plugin/src/esm/records/mod.rs:799-801` (catch-all skip)
- **Status**: NEW
- **Description**: Five record types are present at top level in FalloutNV.esm, gate active gameplay subsystems, and have no current dispatch:

| Record | Count (likely) | Subsystem | Why it matters for FNV |
|---|---:|---|---|
| **`PROJ`** | ~150-300 | Projectiles | Every weapon (WEAP) references a PROJ for muzzle velocity, damage, AoE radius, lifetime, gravity, impact behavior. Without dispatch, the weapon-to-PROJ link can't resolve — weapon firing simulation is blocked. |
| **`IMOD`** | ~100-200 | Item Mods | FNV's signature weapon-mod system (sights, suppressors, extended mags, scopes). Each WEAP has a `mod1`/`mod2`/`mod3` slot referencing IMOD records. Without dispatch, no weapon can be modded. |
| **`EFSH`** | ~100 | Effect Shader | Visual effects for spells, grenade flashes, weapon muzzle flashes, blood splatter. Referenced from MGEF/SPEL/EXPL. Without dispatch, particle / VFX records dangle. |
| **`ARMA`** | ~700+ | Armor Addon | Biped slot variants per race (head, hands, body, feet, etc.). FNV's armor pipeline reads ARMO → ARMA → race-specific MODL. Without dispatch, armor on non-default-race NPCs may render the wrong slot model. |
| **`BPTD`** | ~50 | Body Part Data | Per-NPC dismemberment routing (head, torso, limbs) + biped slot count. Without BPTD, combat damage location reporting and dismemberment effects can't fire. |

- **Evidence**:
  ```bash
  # GRUP scan of FalloutNV.esm vs dispatched arms
  $ python3 scan_grups.py FalloutNV.esm | sort > /tmp/fnv_grups
  $ grep -oE 'b"[A-Z_]{4}"' crates/plugin/src/esm/records/mod.rs | sort -u > /tmp/dispatched
  $ comm -23 /tmp/fnv_grups /tmp/dispatched
  ALOC
  AMEF
  ANIO
  ARMA       ← HIGH
  ASPC
  BPTD       ← HIGH
  CAMS
  ...
  EFSH       ← HIGH
  ...
  IMOD       ← HIGH
  ...
  PROJ       ← HIGH
  ...
  ```
  43 total types not dispatched; 5 are HIGH-impact for FNV gameplay.
- **Trigger Conditions**: Loading any FNV cell that contains a NPC (ARMA / BPTD), a weapon-bearing actor (PROJ link), an explosion-capable munition (EXPL → PROJ), an effect-shader-producing surface (EFSH), or a moddable weapon (IMOD).
- **Impact**:
  - **PROJ**: Weapon firing simulation is blocked at the per-shot level. Today no consumer is wired (the engine doesn't simulate firing yet), but every PROJ-related parameter (gravity, lifetime, radius) needed for the eventual firing path is dropped on parse.
  - **IMOD**: Mod-attached weapons (Cowboy Repeater + extended magazine, Hunting Rifle + scope, etc.) can't resolve their attached IMODs. Vanilla content has ~100-200 IMOD records.
  - **EFSH**: Effect shaders for grenades, mines, melee impact halos, plasma weapon plasma plumes, etc. Without dispatch the visual-effect spawn at MGEF activation has no payload to consume.
  - **ARMA**: Most pressing — drives ARMO → biped slot resolution. Default-race armor (NCR uniforms, raider gear on Caucasian male) often works because the default ARMA happens to be at slot 0 of the ARMO's `mod_arma` array, but Khajiit-equivalent / Argonian-equivalent / mutant content (Vipers, Ghouls, Super Mutants) needs ARMA dispatch for correct slot routing.
  - **BPTD**: NPC dismemberment + per-limb damage location (used by VATS targeting and gore effects). Without BPTD, "shoot Boone in the leg" routes damage generically instead of to the correct limb.
- **Suggested Fix**: Add a dispatch arm for each of the 5 records following the existing pattern at `mod.rs:740-797`. For example:

  ```rust
  // PROJ — projectile records. Every WEAP references a PROJ for
  // muzzle velocity, damage, AoE radius, lifetime, gravity, impact
  // behavior. Stub form (EDID + DATA) sufficient for first-pass
  // wiring; full DATA struct decode lands with the firing simulator.
  b"PROJ" => extract_records(&mut reader, end, b"PROJ", &mut |fid, subs| {
      index.projectiles.insert(fid, parse_proj(fid, subs));
  })?,
  ```

  Each new arm needs:
  - A typed-record parser at `crates/plugin/src/esm/records/<record>.rs`
  - Storage on `EsmIndex` (a new `HashMap<u32, ProjRecord>` field)
  - A row in the `categories()` table at `mod.rs:226` so `total()` and `category_breakdown()` see the new category
  - Optional integration test floor at `crates/plugin/tests/parse_real_esm.rs` (e.g., `assert!(index.projectiles.len() > 100)`)

  Bundle all 5 in a single PR — same scaffolding shape, same test pattern. Estimated ~300 lines of plumbing.
- **Related**:
  - Pattern matches #629 / #630 / #631 / #519 / #520 / #521 closeouts; familiar shape.
  - `#570 SK-D3-03` (open) — `MaterialInfo::material_kind` u8 truncation; once IMOD lands and a consumer reads weapon-mod material variants, may surface as a real bug.

### MEDIUM

#### FNV-D2-NEW-02 — 7 supporting record types affecting NPC AI / crafting / FNV-core systems

- **Severity**: MEDIUM
- **Dimension**: ESM Record Parser
- **Location**: same catch-all skip as FNV-D2-NEW-01
- **Status**: NEW
- **Description**: Additional 7 records gate FNV-relevant subsystems but with smaller blast radius:

| Record | Subsystem | Impact |
|---|---|---|
| **`COBJ`** | Constructible Object (FNV crafting) | Workbench / reloading bench / campfire recipes. Without COBJ, crafting feature is empty. |
| **`IDLE`** | Idle Animations | NPC behavior tree references — "lean against wall", "smoke", "drink", etc. Without IDLE, the AI scheduler picks generic standing pose. |
| **`CSTY`** | Combat Style | Per-NPC combat AI profile (aggression, stealth preference, ranged-vs-melee). Without CSTY, NPCs fall back to engine-default combat behavior. |
| **`IPCT`** + **`IPDS`** | Impact Data + Impact Set | Bullet impact effects (puff of dust on stone, splinters on wood, water splash, blood spray on flesh). |
| **`EXPL`** | Explosion | Frag grenades, mines, explosive ammo blast effects. Links PROJ→EXPL→EFSH. |
| **`REPU`** | Reputation | FNV-CORE: NCR / Legion / Powder Gangers / faction reputation tracking. Drives quest gating and NPC dialogue. ~12 vanilla REPU records. |

- **Evidence**: same GRUP scan output as FNV-D2-NEW-01.
- **Trigger Conditions**: Once each subsystem's renderer / simulation consumer wires up. None blocking today.
- **Impact**: Same per-record additions as FNV-D2-NEW-01, smaller individual blast radius. Bundle for the same reason — uniform plumbing pattern, single PR.
- **Suggested Fix**: Bundle with FNV-D2-NEW-01. Same scaffolding shape per record. REPU is the smallest (12 records) and could ship first as a "plumbing-validates-the-pattern" test case.

### LOW

#### FNV-D2-NEW-03 — 31 supporting record types in the long tail

- **Severity**: LOW
- **Dimension**: ESM Record Parser
- **Location**: same catch-all skip
- **Status**: NEW
- **Description**: 31 remaining record types fall into one of three categories:

  **Audio / supporting metadata (10)**:
  ALOC (audio location), ANIO (animation object), ASPC (acoustic space), CAMS (camera shot), CPTH (camera path), DOBJ (default object), MICN (menu icon), MSET (media set), MUSC (music type), SOUN (sound), VTYP (voice type)

  **Visual / world-building (8)**:
  AMEF (ammo effect), DEBR (debris), GRAS (grass), IMAD (imagespace modifier), LSCR (load screen), LSCT (load screen type), PWAT (placeable water), RGDL (ragdoll)

  **FNV Hardcore mode (4)**:
  DEHY (dehydration), HUNG (hunger), RADS (radiation stages), SLPD (sleep deprivation)

  **FNV Caravan + Casino (6)**:
  CCRD (caravan card), CDCK (caravan deck), CHIP (poker chip), CMNY (caravan money), CHAL (challenge), CSNO (casino)

  **Recipes / crafting (3)**:
  RCCT (recipe category), RCPE (recipe — superseded by COBJ in MEDIUM; FNV ships both)

- **Trigger Conditions**: None block any current rendering or simulation.
- **Impact**: Cumulative — none of these is *individually* important enough to file as MEDIUM, but the long tail keeps the dispatch coverage at 58/101 instead of approaching parity. Each record represents authored content the engine ignores.
- **Suggested Fix**: Defer until a concrete consumer needs them. The dispatch-coverage progression has been driven by consumers (#629 ENCH driven by perk effect simulation, #631 INFO driven by quest dialogue, #634 EsmIndex driven by category telemetry). Same model for the long tail — file individual issues as consumers arrive.

  Three highest-priority subsets within the 31:
  1. **Audio** (ALOC, SOUN, MUSC, VTYP, MSET) — wire when audio backend lands (M??).
  2. **Hardcore mode** (DEHY, HUNG, RADS, SLPD) — wire when survival-meter UI lands.
  3. **Caravan** (CCRD, CDCK, CHIP, CMNY, CHAL) — wire when minigame shipping is on the roadmap.

- **Related**:
  - LSCR / LSCT (load screens) — currently ignored; matters when a "loading" UI overlay is wired.
  - GRAS — grass placement; currently no consumer (M32 terrain LOD splatting handles base textures via LTEX).
  - The pattern of "parser ready, no consumer" is the same as #780 (PERF-N1 — material dedup ratio telemetry waiting for a consumer).

---

## Verified Working — Detailed

### Coverage matrix (58 dispatched / 101 total)

Top-level GRUPs in FalloutNV.esm + dispatch status:

| ✓ Dispatched (58) | ✗ Not dispatched (43) |
|---|---|
| ACTI ADDN ALCH AMMO ARMO AVIF BOOK CELL CLAS CLMT CONT CREA DIAL DOOR ECZN ENCH EYES FACT FLST FURN GLOB GMST HAIR HDPT IDLM IMGS INGR KEYM LGTM LIGH LTEX LVLC LVLI LVLN MESG MGEF MISC MSTT NAVI NAVM NOTE NPC_ PACK PERK QUST RACE REGN SCOL SCPT SPEL STAT TACT TERM TREE TXST WATR WRLD WTHR | **HIGH**: ARMA BPTD EFSH IMOD PROJ • **MEDIUM**: COBJ CSTY EXPL IDLE IPCT IPDS REPU • **LOW (audio)**: ALOC ANIO ASPC CAMS CPTH DOBJ MICN MSET MUSC SOUN VTYP • **LOW (visual)**: AMEF DEBR GRAS IMAD LSCR LSCT PWAT RGDL • **LOW (HC)**: DEHY HUNG RADS SLPD • **LOW (Caravan)**: CCRD CDCK CHAL CHIP CMNY CSNO • **LOW (recipes)**: RCCT RCPE |

### Spot-check pinning (per audit checklist)

- **Varmint Rifle (00004337)**: WEAP record decodes through `extract_records_with_modl` at `mod.rs:509`. AmmoType (`AMMO 00004244` = `5mm round`) resolves via the AMMO dispatch arm. Skill bonus (`Guns`) routes through AVIF.
- **NCR faction (000F43DD)**: FACT record post-#481/#482/#483 has correct `data` byte (`0x00` = neutral) and XNAM combat reactions. 4 listed allied factions (NCRSecurity, Vault21, NewVegas, Followers) and 6 hostile (Legion, etc.).
- **VATS AVIF (any 0001A0xx)**: AVIF dispatched via `parse_avif` at `mod.rs:762`. `index.actor_values.len() == 76` on FalloutNV.esm.

### Pattern checks

- `unreachable_patterns` at the historical `cell.rs:211` site: closed via #378 on 2026-04-20. `cargo build --release` warning-free.
- FO4 record dispatch (`SCOL`/`MOVS`/`PKIN`/`TXST`) is at top level of `parse_esm_with_load_order`, not inside the STAT-like multi-match — no risk of stealing FNV STAT/MSTT/FURN/DOOR/LIGH/FLOR/TREE/IDLM/BNDS/ADDN/TACT records.
- CELL `XCLL` `fog_far_color`: correctly gated on `sub.data.len() >= 92` (Skyrim+ extended layout); FNV's 40-byte XCLL produces `fog_far_color: None`. Test pinning at `cell/tests.rs:1657` (`assert!(lit.fog_far_color.is_none())`).

---

## Prioritized Fix Order

1. **FNV-D2-NEW-01 HIGH** (PROJ + IMOD + EFSH + ARMA + BPTD bundle) — single PR. Each record gets ~60 lines of scaffolding (parser + struct + index field + categories row + integration-test floor). Estimated 300 lines total. Pattern matches every prior FNV-D2 closeout.

2. **FNV-D2-NEW-02 MEDIUM** (COBJ + IDLE + CSTY + IPCT + IPDS + EXPL + REPU bundle) — same shape; starts with REPU as the smallest validation case (12 records).

3. **FNV-D2-NEW-03 LOW** — defer; file individual issues per record as consumers arrive (M?? audio backend, M?? hardcore UI, M?? Caravan minigame, M?? load-screen system, etc.).

The fix order matches the prior FNV-D2 progression: PR per ~5-7 records, ratchet coverage from 58/101 → 70/101 → 80/101 over ~3 PRs. Coverage will plateau at ~85-90/101 once the long tail's consumer dependencies clear.

---

## Methodology Notes

- GRUP scan of FalloutNV.esm via a 30-line Python helper at `/tmp/scan_grups.py` (deleted after audit). Walks the file at depth 0, collects every top-level GRUP label.
- Dispatched arms enumerated via `grep -oE 'b"[A-Z_]{4}"' crates/plugin/src/esm/records/mod.rs | sort -u`.
- Set difference (`comm -23`) produces the 43-row gap.
- Live record counts captured via `cargo test -p byroredux-plugin --test parse_real_esm --release parse_rate_fnv_esm -- --ignored --nocapture` against the actual on-disk FalloutNV.esm.
- Sub-agent dispatches deliberately not used per established methodology — direct main-context delta audit produces a deterministic deliverable.
- The audit prompt's "23 record types" framing is stale (predates the 2026-04-21 / 2026-04-24 audits' closures). Current dispatch is 58 record types covering 64 446 typed records.

---

*Generated by `/audit-fnv --focus 2` on 2026-05-03. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_FNV_2026-05-03_DIM2.md`.*
