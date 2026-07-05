//! Per-game ESM record-count integration tests.
//!
//! Mirrors the `crates/nif/tests/parse_real_nifs.rs` pattern: walk a
//! real game's master file, assert the total parsed-record count stays
//! at or above the M24 Phase 1 baseline, and sanity-check the
//! per-category floors. `#[ignore]`-gated because they require real
//! game data (CI has none). Opt in with:
//!
//! ```sh
//! cargo test -p byroredux-plugin --test parse_real_esm -- --ignored
//! ```
//!
//! Override the `BYROREDUX_*_DATA` env vars to point at a non-default
//! install path (see the `data_dir` helper below for defaults).
//!
//! See issue #488 — pre-existing inline test at
//! `records/mod.rs::tests::parse_real_fnv_esm_record_counts` was
//! hardcoded-path only and had no `total >= 13_684` floor.

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::reader::GameKind;
use std::path::PathBuf;

/// Resolve a `Data/` directory from an env var, falling back to the
/// canonical Steam install path on the dev machine. Returns `None` when
/// neither resolves — the test then skips cleanly. Mirrors the pattern
/// from `crates/nif/tests/common/mod.rs::game_data_dir`.
fn data_dir(env_var: &str, fallback: &str) -> Option<PathBuf> {
    if let Ok(v) = std::env::var(env_var) {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        eprintln!("{env_var} points to {v:?} which is not a directory; falling back to default");
    }
    let p = PathBuf::from(fallback);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// FNV: 60,000 records floor — covers the 13,684 M24 Phase 1 baseline
/// plus the 7 categories added in #446/#447 (PACK 4163, QUST 436,
/// DIAL 18215, MESG 1144, PERK 176, SPEL 270, MGEF 289 = +24,693).
/// Observed 2026-04: 62,219. Floor sits a few percent below to absorb
/// DLC patch drift without masking regressions.
const FNV_TOTAL_FLOOR: usize = 60_000;

/// FO3: 30,000 records floor — covers the original 18,007 baseline +
/// the 7 categories added in #446/#447. Observed 2026-04: 31,101.
const FO3_TOTAL_FLOOR: usize = 30_000;

/// FO4: 70,000 records floor — observed 2026-05-04 on vanilla
/// Fallout4.esm: 76,468 (with #817 categories landed: cells 964 +
/// statics 31,989 + scols 2,617 + packins 872 + material_swaps 2,537 +
/// texture_sets 379 + items 4,076 + NPCs 3,015 + game_settings 2,039
/// + globals 1,346 + LVLI 2,098 + factions 699 + weathers 71 + many
/// smaller categories). Floor at 70 K absorbs DLC/patch drift without
/// masking a category-wipe regression.
const FO4_TOTAL_FLOOR: usize = 70_000;

#[test]
#[ignore]
fn parse_rate_fnv_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[FNV] skipping: BYROREDUX_FNV_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("FalloutNV.esm");
    let bytes = std::fs::read(&esm_path).expect("read FalloutNV.esm");
    let parse_start = std::time::Instant::now();
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");
    let parse_elapsed = parse_start.elapsed();
    // #527 — fused single-pass walker. Pre-fix audit baseline was
    // 1.21s release on a cold load (two full walks of the 70 MB
    // ESM); post-fix observed ~1.095s. The timing is diagnostic
    // only — too disk-cache-sensitive to assert against without
    // dedicated bench infra. The functional baselines below catch
    // any regression that would matter to consumers.
    eprintln!("[FNV] parse_esm wall={:?}", parse_elapsed);

    eprintln!(
        "[FNV] total={} | items={} containers={} LVLI={} LVLN={} NPCs={} \
         races={} classes={} factions={} globals={} game_settings={} \
         packages={} quests={} dialogues={} messages={} perks={} \
         spells={} magic_effects={} activators={} terminals={} form_lists={} \
         projectiles={} effect_shaders={} item_mods={} armor_addons={} body_parts={} \
         reputations={} explosions={} combat_styles={} idle_animations={} \
         impacts={} impact_data_sets={} recipes={} trees={}",
        index.total(),
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.npcs.len(),
        index.races.len(),
        index.classes.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
        index.packages.len(),
        index.quests.len(),
        index.dialogues.len(),
        index.messages.len(),
        index.perks.len(),
        index.spells.len(),
        index.magic_effects.len(),
        index.activators.len(),
        index.terminals.len(),
        index.form_lists.len(),
        index.projectiles.len(),
        index.effect_shaders.len(),
        index.item_mods.len(),
        index.armor_addons.len(),
        index.body_parts.len(),
        index.reputations.len(),
        index.explosions.len(),
        index.combat_styles.len(),
        index.idle_animations.len(),
        index.impacts.len(),
        index.impact_data_sets.len(),
        index.recipes.len(),
        index.trees.len(),
    );

    // Primary M24 baseline assertion — the "13,684 structured records"
    // claim that the ROADMAP, CLAUDE.md, and the FNV audit all cite.
    // Covers every EsmIndex category (items, containers, LVLI/LVLN, NPCs,
    // races, classes, factions, globals, game_settings, weathers,
    // climates, scripts, supplementary records, cells + statics).
    assert!(
        index.total() >= FNV_TOTAL_FLOOR,
        "FNV total {} < M24 Phase 1 baseline {}",
        index.total(),
        FNV_TOTAL_FLOOR,
    );

    // Per-category floors — mirror the existing inline test at
    // records/mod.rs:525-574 so a single-category regression fails
    // loud even when the total stays above the overall floor.
    assert!(index.items.len() > 2500, "items={}", index.items.len());
    assert!(
        index.containers.len() > 2000,
        "containers={}",
        index.containers.len(),
    );
    assert!(
        index.leveled_items.len() > 2000,
        "LVLI={}",
        index.leveled_items.len(),
    );
    assert!(
        index.leveled_npcs.len() > 250,
        "LVLN={}",
        index.leveled_npcs.len(),
    );
    assert!(index.npcs.len() > 3000, "NPCs={}", index.npcs.len());
    assert!(
        index.factions.len() > 500,
        "factions={}",
        index.factions.len()
    );
    assert!(
        index.game_settings.len() > 500,
        "game_settings={}",
        index.game_settings.len(),
    );

    // Floors for the 7 categories added in #446/#447. Observed FNV
    // counts: packages=4163, quests=436, dialogues=18215, messages=1144,
    // perks=176, spells=270, magic_effects=289. Each floor sits a few
    // percent below.
    assert!(index.packages.len() > 4000, "PACK={}", index.packages.len());
    assert!(index.quests.len() > 400, "QUST={}", index.quests.len());
    assert!(
        index.dialogues.len() > 17_000,
        "DIAL={}",
        index.dialogues.len(),
    );
    assert!(index.messages.len() > 1000, "MESG={}", index.messages.len(),);
    assert!(index.perks.len() > 150, "PERK={}", index.perks.len());
    assert!(index.spells.len() > 250, "SPEL={}", index.spells.len());
    assert!(
        index.magic_effects.len() > 270,
        "MGEF={}",
        index.magic_effects.len(),
    );

    // ACTI / TERM floors (#521). Issue body estimated ≥1500/≥400;
    // reference run on vanilla FNV (no DLC) observes 1143/344 —
    // the audit's estimates included DLC content that isn't in a
    // fresh Steam install. Floors sit a few percent below the
    // observed vanilla numbers to absorb cell-group-skip edge cases
    // without masking a dispatch regression.
    assert!(
        index.activators.len() >= 1000,
        "ACTI={} (expected >= 1000; vanilla ships 1143)",
        index.activators.len(),
    );
    assert!(
        index.terminals.len() >= 300,
        "TERM={} (expected >= 300; vanilla ships 344)",
        index.terminals.len(),
    );

    // #630 / audit FNV-D2-02 regression guard: FLST FormID lists must
    // dispatch end-to-end. Pre-fix the entire top-level group fell
    // through to the catch-all skip and every `IsInList <flst>` perk
    // condition / Caravan deck lookup hit an empty map. Vanilla
    // FalloutNV.esm ships ~340 FLST records; floor at 250 absorbs the
    // BSA-vs-loose-files edge case without masking a dispatch
    // regression. At least one FLST must carry > 1 entry — an
    // EDID-only FLST with empty entries is the parse-side indicator
    // of a sub-record extraction regression.
    assert!(
        index.form_lists.len() >= 250,
        "FLST={} (expected >= 250; vanilla ships ~340)",
        index.form_lists.len(),
    );

    // SpeedTree Phase 1.1 / TREE record dispatch. Pre-fix TREE collapsed
    // into the generic MODL-only path, dropping ICON / SNAM / CNAM /
    // BNAM / PFIG silently. Vanilla FNV ships 3 TREE bases (Joshua tree,
    // creosote, dead tree); the floor at >= 1 absorbs DLC-only TREE
    // additions without masking a dispatch regression. Each must have a
    // non-empty model_path that ends in `.spt` — that's the SpeedTree
    // route the cell loader will eventually branch on.
    assert!(
        !index.trees.is_empty(),
        "TREE={} — every FNV exterior tree REFR points at a TREE base; \
         the dispatch must produce at least one entry",
        index.trees.len(),
    );
    let spt_trees = index
        .trees
        .values()
        .filter(|t| t.has_speedtree_binary())
        .count();
    assert_eq!(
        spt_trees,
        index.trees.len(),
        "every vanilla FNV TREE points at a `.spt` — found {}/{} routed \
         through the SpeedTree path",
        spt_trees,
        index.trees.len(),
    );
    let flst_with_entries = index
        .form_lists
        .values()
        .filter(|f| f.entries.len() > 1)
        .count();
    assert!(
        flst_with_entries >= 100,
        "FLSTs with >1 entry = {}/{} — pre-fix 0/0 because the group \
         was skipped; expected >= 100 after #630",
        flst_with_entries,
        index.form_lists.len(),
    );

    // #808 / audit FNV-D2-NEW-01 regression guard: 5 gameplay-critical
    // record types must dispatch end-to-end. Pre-fix each top-level
    // group fell through to the catch-all skip and the entire
    // category lookup returned an empty map. Floors below sit a few
    // percent under the observed vanilla counts so a dispatch
    // regression fails loud while ordinary content drift doesn't.
    //
    // Observed vanilla counts (FalloutNV.esm, no DLC, 2026-05-03):
    //   PROJ=95, EFSH=35, IMOD=50, ARMA=131, BPTD=49.
    // The audit body's "150-300 / 100 / 100-200 / 700+" estimates were
    // inflated against the FO3+FNV+DLC superset; vanilla FNV ships
    // smaller numbers. DLC content will push these up.
    assert!(
        index.projectiles.len() >= 80,
        "PROJ={} (expected >= 80; vanilla ships ~95)",
        index.projectiles.len(),
    );
    assert!(
        index.effect_shaders.len() >= 30,
        "EFSH={} (expected >= 30; vanilla ships ~35)",
        index.effect_shaders.len(),
    );
    assert!(
        index.item_mods.len() >= 40,
        "IMOD={} (expected >= 40; vanilla ships ~50)",
        index.item_mods.len(),
    );
    assert!(
        index.armor_addons.len() >= 110,
        "ARMA={} (expected >= 110; vanilla ships ~131)",
        index.armor_addons.len(),
    );
    assert!(
        index.body_parts.len() >= 40,
        "BPTD={} (expected >= 40; vanilla ships ~49)",
        index.body_parts.len(),
    );

    // At least one PROJ must have a parsed muzzle_speed > 0 — proves
    // the DATA sub-record decode fires, not just the EDID extraction.
    let projs_with_speed = index
        .projectiles
        .values()
        .filter(|p| p.muzzle_speed > 1.0)
        .count();
    assert!(
        projs_with_speed >= 60,
        "PROJ with muzzle_speed > 0 = {}/{}, expected >= 60 (DATA \
         decode regression)",
        projs_with_speed,
        index.projectiles.len(),
    );

    // At least one ARMA must have non-zero biped_flags — proves the
    // BMDT decode fires. ARMOs with zero biped flags exist (the all-
    // race-default ARMA from a few records) but most ARMAs have a
    // body region set.
    let arma_with_biped = index
        .armor_addons
        .values()
        .filter(|a| a.biped_flags != 0)
        .count();
    assert!(
        arma_with_biped >= 100,
        "ARMA with non-zero biped_flags = {}/{}, expected >= 100 \
         (BMDT decode regression)",
        arma_with_biped,
        index.armor_addons.len(),
    );

    // #809 / audit FNV-D2-NEW-02 regression guard: 7 supporting record
    // types must dispatch end-to-end. Pre-fix each fell through to the
    // catch-all skip.
    //
    // Observed vanilla counts (FalloutNV.esm, no DLC, 2026-05-03):
    //   REPU=13, EXPL=154, CSTY=84, IDLE=1597, IPCT=125, IPDS=60, COBJ=0.
    //
    // COBJ=0 is intentional — vanilla FNV's crafting system predates
    // the COBJ-driven recipe table (FO3 introduces the type but FNV
    // workbenches use script effects, not COBJ records). DLC content
    // (Honest Hearts, Old World Blues, Lonesome Road) adds some COBJs
    // but vanilla ships an empty group. Floor at 0 documents this.
    assert!(
        index.reputations.len() >= 10,
        "REPU={} (expected >= 10; vanilla ships ~13)",
        index.reputations.len(),
    );
    assert!(
        index.explosions.len() >= 130,
        "EXPL={} (expected >= 130; vanilla ships ~154)",
        index.explosions.len(),
    );
    assert!(
        index.combat_styles.len() >= 70,
        "CSTY={} (expected >= 70; vanilla ships ~84)",
        index.combat_styles.len(),
    );
    assert!(
        index.idle_animations.len() >= 1400,
        "IDLE={} (expected >= 1400; vanilla ships ~1597)",
        index.idle_animations.len(),
    );
    assert!(
        index.impacts.len() >= 100,
        "IPCT={} (expected >= 100; vanilla ships ~125)",
        index.impacts.len(),
    );
    assert!(
        index.impact_data_sets.len() >= 50,
        "IPDS={} (expected >= 50; vanilla ships ~60)",
        index.impact_data_sets.len(),
    );
    // COBJ vanilla=0 — dispatch arm is in place; DLC content adds some.

    // At least one EXPL must have parsed damage > 0 — proves the DATA
    // sub-record decode fires.
    let expls_with_damage = index.explosions.values().filter(|e| e.damage > 0.0).count();
    assert!(
        expls_with_damage >= 100,
        "EXPL with damage > 0 = {}/{}, expected >= 100 (DATA decode \
         regression)",
        expls_with_damage,
        index.explosions.len(),
    );

    // At least one CSTY must have non-zero csty_flags — proves the
    // CSTD sub-record decode fires.
    let csty_with_flags = index
        .combat_styles
        .values()
        .filter(|c| c.csty_flags != 0)
        .count();
    assert!(
        csty_with_flags >= 50,
        "CSTY with non-zero flags = {}/{}, expected >= 50 (CSTD decode \
         regression)",
        csty_with_flags,
        index.combat_styles.len(),
    );

    // At least one IDLE must have a non-empty animation_path — proves
    // MODL extraction fires.
    let idle_with_path = index
        .idle_animations
        .values()
        .filter(|i| !i.animation_path.is_empty())
        .count();
    assert!(
        idle_with_path >= 1000,
        "IDLE with animation_path = {}/{}, expected >= 1000 (MODL \
         extraction regression)",
        idle_with_path,
        index.idle_animations.len(),
    );

    // #810 / audit FNV-D2-NEW-03 regression guard: the 31 long-tail
    // record types must dispatch end-to-end via `parse_minimal_esm_record`.
    // Pre-fix each fell through the catch-all skip. Granular per-record
    // floors aren't worth the test churn — when a real consumer arrives
    // and a record gains its own dedicated parser via the #808/#809
    // pattern, the per-record floor lands with that work. Instead pin
    // the SUM as a single anti-regression guard: vanilla FNV ships 5000+
    // records across the long tail (1000+ SOUN alone), so a count below
    // 1000 means the dispatch arms aren't firing.
    let long_tail_total: usize = index.audio_locations.len()
        + index.animation_objects.len()
        + index.acoustic_spaces.len()
        + index.camera_shots.len()
        + index.camera_paths.len()
        + index.default_objects.len()
        + index.menu_icons.len()
        + index.media_sets.len()
        + index.music_types.len()
        + index.sounds.len()
        + index.voice_types.len()
        + index.ammo_effects.len()
        + index.debris.len()
        + index.grasses.len()
        + index.imagespace_modifiers.len()
        + index.load_screens.len()
        + index.load_screen_types.len()
        + index.placeable_waters.len()
        + index.ragdolls.len()
        + index.dehydration_stages.len()
        + index.hunger_stages.len()
        + index.radiation_stages.len()
        + index.sleep_deprivation_stages.len()
        + index.caravan_cards.len()
        + index.caravan_decks.len()
        + index.challenges.len()
        + index.poker_chips.len()
        + index.caravan_money.len()
        + index.casinos.len()
        + index.recipe_categories.len()
        + index.recipe_records.len();
    assert!(
        long_tail_total >= 1000,
        "long-tail total = {} (expected >= 1000; vanilla FNV ships ~5500 \
         across the 31 record types — most of that is SOUN). A count \
         this low means the dispatch arms aren't firing.",
        long_tail_total,
    );

    // SOUN is the largest single contributor (~1100 vanilla); pin a
    // stand-alone floor so a SOUN-specific dispatch regression fails
    // loud independently of the other 30 records.
    assert!(
        index.sounds.len() >= 800,
        "SOUN={} (expected >= 800; vanilla ships ~1100)",
        index.sounds.len(),
    );

    eprintln!(
        "[FNV] long-tail total = {} | sounds={} idle={} grasses={} debris={}",
        long_tail_total,
        index.sounds.len(),
        index.idle_animations.len(),
        index.grasses.len(),
        index.debris.len(),
    );

    // #533 / audit M33-01 regression guard: at least one FNV weather must
    // have a non-zero NAM0 sky colour. Pre-fix the `>= 240 B` gate dropped
    // ~12/63 FNV weathers silently (those using the 160-B stride). Weather
    // count floor: FNV ships ≥50 WTHRs and at least the common-case ones
    // (e.g. NVWastelandClear*) must parse.
    assert!(
        index.weathers.len() >= 50,
        "FNV weathers={} (expected >= 50)",
        index.weathers.len(),
    );
    let nonzero_nam0 = index
        .weathers
        .values()
        .filter(|w| {
            let c = w.sky_colors[0][1]; // SKY_UPPER / TOD_DAY
            c.r != 0 || c.g != 0 || c.b != 0
        })
        .count();
    assert!(
        nonzero_nam0 >= 40,
        "FNV non-zero-NAM0 weathers={}/{}, expected >= 40",
        nonzero_nam0,
        index.weathers.len(),
    );

    // #534 / audit M33-02 regression guard: cloud texture sub-records
    // live in DNAM/CNAM/ANAM/BNAM (not 00TX-03TX). Pre-fix the parser
    // populated zero cloud textures across every WTHR in every shipped
    // master. FNV weathers near-universally ship DNAM (layer 0) per the
    // FourCC histogram (63/63 in vanilla).
    let with_layer_0 = index
        .weathers
        .values()
        .filter(|w| {
            w.cloud_textures[0]
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_some()
        })
        .count();
    assert!(
        with_layer_0 >= 50,
        "FNV weathers with cloud layer 0 = {}/{} — pre-fix 0/63; \
         expected >= 50 after #534",
        with_layer_0,
        index.weathers.len(),
    );

    // #536 / audit M33-04 regression guard: FNV FNAM fog parsing.
    // Pre-fix every FNV weather defaulted to `fog_day_far = 10000.0`
    // because the FNAM arm body was empty (comment claimed "fallback
    // when HNAM is absent" but FNV has no HNAM). Count weathers with
    // any non-default fog field as proof the body now fires.
    let with_nondefault_fog = index
        .weathers
        .values()
        .filter(|w| {
            (w.fog_day_far - 10000.0).abs() > 0.1
                || w.fog_day_near != 0.0
                || (w.fog_night_far - 10000.0).abs() > 0.1
                || w.fog_night_near != 0.0
        })
        .count();
    assert!(
        with_nondefault_fog >= 50,
        "FNV weathers with non-default fog = {}/{} — pre-fix 0/63; \
         expected >= 50 after #536",
        with_nondefault_fog,
        index.weathers.len(),
    );

    // #538 regression guard: classification at DATA byte 11. Find the
    // canonical `NVWastelandClear` and confirm its classification flag
    // is `WTHR_PLEASANT`. Pre-fix the parser read byte 13 (padding) and
    // returned `0x00` for this record.
    let clear = index
        .weathers
        .values()
        .find(|w| w.editor_id == "NVWastelandClear")
        .expect("NVWastelandClear should parse");
    assert_eq!(
        clear.classification,
        byroredux_plugin::esm::records::weather::WTHR_PLEASANT,
        "NVWastelandClear should classify as PLEASANT; got 0x{:02X}",
        clear.classification,
    );

    // #1538 regression guard: SCOL (static collections) must parse for FNV.
    // The `is_fo4_plus` gate wrongly treated SCOL as FO4-only and skipped
    // the whole GRUP, dropping all 98 FalloutNV.esm SCOL bases — 1084 REFRs
    // (road segments, guardrails, debris LOD clusters) then mis-resolved to
    // nothing. SCOL is a Gamebryo-Fallout record (FO3 54, FNV 98); the gate
    // now admits Fallout3NV. Exact count pins the parse, not a floor.
    assert_eq!(
        index.cells.scols.len(),
        98,
        "FNV must parse exactly 98 SCOL bases (pre-#1538 the is_fo4_plus \
         gate skipped the whole GRUP, leaving 0); got {}",
        index.cells.scols.len(),
    );
}

#[test]
#[ignore]
fn parse_rate_fo3_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_FO3_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
    ) else {
        eprintln!("[FO3] skipping: BYROREDUX_FO3_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("Fallout3.esm");
    let bytes = std::fs::read(&esm_path).expect("read Fallout3.esm");
    let index = parse_esm(&bytes).expect("parse Fallout3.esm");

    eprintln!(
        "[FO3] total={} | items={} containers={} LVLI={} LVLN={} LVLC={} \
         NPCs={} creatures={} factions={} globals={} game_settings={} \
         scripts={} trees={}",
        index.total(),
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.leveled_creatures.len(),
        index.npcs.len(),
        index.creatures.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
        index.scripts.len(),
        index.trees.len(),
    );

    // `index.total()` sums the ~95 typed category maps (index.rs::total),
    // a subset of the file's structured records — NOT the raw record count.
    // Observed 2026-04 on the GOTY master: 31,101; FO3_TOTAL_FLOOR (30,000)
    // sits just below it to absorb patch drift without masking regressions.
    // (The stale "18,007 records" from AUDIT_FO3_2026-04-19 predates the
    // #446/#447 category additions — see the FO3_TOTAL_FLOOR const doc.)
    // Distinct from the *file* baseline re-verified 2026-05-26: 44,657 total
    // = 37,459 structured + 7,198 NAVM.
    assert!(
        index.total() >= FO3_TOTAL_FLOOR,
        "FO3 total {} < audit baseline {}",
        index.total(),
        FO3_TOTAL_FLOOR,
    );

    // FO3-specific record categories — CREA + LVLC + SCPT resolve
    // regressions around the FO3 audit fixes (#442, #443, #448).
    assert!(
        index.creatures.len() >= 50,
        "CREA={} — FO3 bestiary must parse per #442",
        index.creatures.len(),
    );
    // LVLC floor reflects observed FO3.esm count (60 vanilla, GOTY
    // patch revision). The audit's "FO3 uses LVLC for most enemies"
    // characterization was off — FO3 actually leans on LVLN like FNV
    // with a small LVLC tail. Keep the floor low to absorb DLC patches
    // without masking a full regression.
    assert!(
        index.leveled_creatures.len() >= 40,
        "LVLC={} — FO3 enemy spawn tables must parse per #448",
        index.leveled_creatures.len(),
    );
    assert!(
        index.scripts.len() >= 500,
        "SCPT={} — pre-Papyrus bytecode records must parse per #443",
        index.scripts.len(),
    );

    // #533 / audit M33-01: FO3 NAM0 is 160 B (not 240). Pre-fix the parser
    // silently dropped every FO3 weather → black sky dome on every
    // exterior. Assert the fix by requiring most weathers (vanilla ships
    // 27 WTHRs; some are stubs like DefaultWeather with zero bytes on
    // disk) to parse to at least one non-zero RGB channel in SKY_UPPER.
    assert!(
        index.weathers.len() >= 20,
        "FO3 weathers={} (expected >= 20)",
        index.weathers.len(),
    );
    let nonzero_nam0 = index
        .weathers
        .values()
        .filter(|w| {
            let c = w.sky_colors[0][1]; // SKY_UPPER / TOD_DAY
            c.r != 0 || c.g != 0 || c.b != 0
        })
        .count();
    assert!(
        nonzero_nam0 >= 15,
        "FO3 non-zero-NAM0 weathers={}/{} — expected >= 15 after #533 fix; \
         pre-fix every weather dropped NAM0 silently",
        nonzero_nam0,
        index.weathers.len(),
    );

    // #534 / audit M33-02: FO3 ships 27 WTHRs, every one has DNAM.
    let with_layer_0 = index
        .weathers
        .values()
        .filter(|w| {
            w.cloud_textures[0]
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_some()
        })
        .count();
    assert!(
        with_layer_0 >= 20,
        "FO3 weathers with cloud layer 0 = {}/{} — expected >= 20 after #534",
        with_layer_0,
        index.weathers.len(),
    );

    // #536 / audit M33-04: FO3 FNAM fog.
    let with_nondefault_fog = index
        .weathers
        .values()
        .filter(|w| {
            (w.fog_day_far - 10000.0).abs() > 0.1
                || w.fog_day_near != 0.0
                || (w.fog_night_far - 10000.0).abs() > 0.1
                || w.fog_night_near != 0.0
        })
        .count();
    assert!(
        with_nondefault_fog >= 15,
        "FO3 weathers with non-default fog = {}/{} — expected >= 15 after #536",
        with_nondefault_fog,
        index.weathers.len(),
    );

    // SpeedTree Phase 1.1 / TREE record dispatch — same shape as the
    // FNV assertion. Vanilla FO3 ships 9 TREE bases (DC swamp foliage
    // + a handful of dead trees). Every one points at a `.spt`.
    assert!(
        !index.trees.is_empty(),
        "FO3 TREE={} — DC swamp / wasteland trees must dispatch",
        index.trees.len(),
    );
    let spt_trees = index
        .trees
        .values()
        .filter(|t| t.has_speedtree_binary())
        .count();
    assert_eq!(
        spt_trees,
        index.trees.len(),
        "every vanilla FO3 TREE points at a `.spt` — found {}/{} routed \
         through the SpeedTree path",
        spt_trees,
        index.trees.len(),
    );
}

/// Oblivion: the 160-byte NAM0 stride target of #533. Minimal parse
/// harness (no per-category floors — that lives in future Oblivion
/// dispatch work) just verifies every NAM0 is read. Observed vanilla
/// 2026-04: 19 CLMTs, 37 WTHRs.
#[test]
#[ignore]
fn parse_rate_oblivion_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[OBL] skipping: BYROREDUX_OBL_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("Oblivion.esm");
    let bytes = std::fs::read(&esm_path).expect("read Oblivion.esm");
    let index = parse_esm(&bytes).expect("parse Oblivion.esm");

    eprintln!(
        "[OBL] total={} | weathers={} climates={} trees={}",
        index.total(),
        index.weathers.len(),
        index.climates.len(),
        index.trees.len(),
    );

    // #533 / audit M33-01: Oblivion NAM0 is 160 B. Same gate failure as
    // FO3 pre-fix — every WTHR silently dropped. Assertion mirrors the
    // FO3 one.
    assert!(
        index.weathers.len() >= 30,
        "OBL weathers={} (expected >= 30)",
        index.weathers.len(),
    );
    let nonzero_nam0 = index
        .weathers
        .values()
        .filter(|w| {
            let c = w.sky_colors[0][1]; // SKY_UPPER / TOD_DAY
            c.r != 0 || c.g != 0 || c.b != 0
        })
        .count();
    assert!(
        nonzero_nam0 >= 25,
        "OBL non-zero-NAM0 weathers={}/{} — expected >= 25 after #533 fix",
        nonzero_nam0,
        index.weathers.len(),
    );

    // #534 / audit M33-02: Oblivion ships 2 cloud layers (DNAM + CNAM).
    // Histogram: DNAM on 35/37 WTHRs.
    let with_layer_0 = index
        .weathers
        .values()
        .filter(|w| {
            w.cloud_textures[0]
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_some()
        })
        .count();
    assert!(
        with_layer_0 >= 25,
        "OBL weathers with cloud layer 0 = {}/{} — expected >= 25 after #534",
        with_layer_0,
        index.weathers.len(),
    );

    // #536 / audit M33-04: Oblivion FNAM is 16 B and carries fog (HNAM
    // is 56 B of *different* lighting-model fields — see #537). Pre-fix
    // the HNAM arm gated on `>= 16` and silently overwrote FNAM's
    // correct fog values with HNAM's first-4-f32 lighting parameters,
    // saturating every Oblivion exterior to `fog_far ≈ 4.0`.
    let with_nondefault_fog = index
        .weathers
        .values()
        .filter(|w| {
            (w.fog_day_far - 10000.0).abs() > 0.1
                || w.fog_day_near != 0.0
                || (w.fog_night_far - 10000.0).abs() > 0.1
                || w.fog_night_near != 0.0
        })
        .count();
    assert!(
        with_nondefault_fog >= 25,
        "OBL weathers with non-default fog = {}/{} — expected >= 25 after #536",
        with_nondefault_fog,
        index.weathers.len(),
    );
    // Sanity bound: no Oblivion weather should come back with
    // `fog_far < 100` (that was the HNAM-clobber footprint).
    let tiny_fog = index
        .weathers
        .values()
        .filter(|w| w.fog_day_far > 0.0 && w.fog_day_far < 100.0)
        .count();
    assert_eq!(
        tiny_fog, 0,
        "OBL weathers with absurd fog_day_far < 100 = {} — \
         pre-fix HNAM clobbered fog_far to ~4.0. Should be 0 after #536.",
        tiny_fog,
    );

    // #538: Oblivion is the cleanest evidence — its vanilla WTHRs span
    // all four flag values. Pin one of each against byte 11.
    use byroredux_plugin::esm::records::weather::{
        WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW,
    };
    for (edid, expected) in &[
        ("Clear", WTHR_PLEASANT),
        ("Cloudy", WTHR_CLOUDY),
        ("Rain", WTHR_RAINY),
        ("Snow", WTHR_SNOW),
    ] {
        let w = index
            .weathers
            .values()
            .find(|w| w.editor_id == *edid)
            .unwrap_or_else(|| panic!("OBL weather '{}' should parse", edid));
        assert_eq!(
            w.classification, *expected,
            "OBL '{}' classification = 0x{:02X}; expected 0x{:02X}",
            edid, w.classification, *expected,
        );
    }

    // SpeedTree Phase 1.1 / TREE record dispatch — Oblivion is the
    // densest forest content in the lineage (vanilla Cyrodiil ships
    // 142 TREE bases for the various oak / pine / birch / etc.
    // species). The floor at >= 100 absorbs DLC trims without
    // masking a regression. Every one points at a `.spt`.
    assert!(
        index.trees.len() >= 100,
        "OBL TREE={} (expected >= 100; vanilla ships 142) — Cyrodiil \
         forests rely entirely on the TREE dispatch",
        index.trees.len(),
    );
    let spt_trees = index
        .trees
        .values()
        .filter(|t| t.has_speedtree_binary())
        .count();
    assert_eq!(
        spt_trees,
        index.trees.len(),
        "every vanilla Oblivion TREE points at a `.spt` — found {}/{} \
         routed through the SpeedTree path",
        spt_trees,
        index.trees.len(),
    );
    // Sanity: at least one TREE carries CNAM (canopy params). Pre-fix
    // CNAM was silently dropped alongside ICON/SNAM/BNAM/PFIG.
    let with_cnam = index
        .trees
        .values()
        .filter(|t| !t.canopy_params.is_empty())
        .count();
    assert!(
        with_cnam >= 100,
        "OBL TREE with CNAM = {}/{} — pre-#TREE every CNAM dropped silently",
        with_cnam,
        index.trees.len(),
    );

    // #966 / OBL-D3-NEW-02 — Oblivion-unique base records that fell
    // through the catch-all skip pre-fix. Floors below vanilla counts
    // so DLC trims / patches don't fail the test, but high enough to
    // catch a dispatch-arm regression.
    eprintln!(
        "[OBL] birthsigns={} clothing={} apparatuses={} sigil_stones={} soul_gems={}",
        index.birthsigns.len(),
        index.clothing.len(),
        index.apparatuses.len(),
        index.sigil_stones.len(),
        index.soul_gems.len(),
    );
    assert!(
        index.birthsigns.len() >= 13,
        "OBL BSGN = {} (expected >= 13 — vanilla ships exactly 13)",
        index.birthsigns.len(),
    );
    assert!(
        index.clothing.len() >= 100,
        "OBL CLOT = {} (expected >= 100 — vanilla ~150)",
        index.clothing.len(),
    );
    assert!(
        index.apparatuses.len() >= 4,
        "OBL APPA = {} (expected >= 4 — vanilla ships 4 tools)",
        index.apparatuses.len(),
    );
    assert!(
        index.sigil_stones.len() >= 10,
        "OBL SGST = {} (expected >= 10)",
        index.sigil_stones.len(),
    );
    assert!(
        index.soul_gems.len() >= 10,
        "OBL SLGM = {} (expected >= 10)",
        index.soul_gems.len(),
    );
    // Every SLGM must surface SLCP soul_capacity > 0 — the audit
    // originally mis-named the field as "DATA byte 0" but the
    // authoritative source is the SLCP sub-record. A zero capacity
    // means the parser silently dropped SLCP again.
    let with_capacity = index
        .soul_gems
        .values()
        .filter(|s| s.soul_capacity > 0)
        .count();
    assert!(
        with_capacity * 2 >= index.soul_gems.len(),
        "at least half of OBL SLGMs should carry SLCP, got {}/{}",
        with_capacity,
        index.soul_gems.len(),
    );
    // Sanity: soul magnitude enums fit in 0..=5.
    for s in index.soul_gems.values() {
        assert!(
            s.soul_capacity <= 5 && s.current_soul <= 5,
            "SLGM '{}' soul enum out of range: capacity={} current={}",
            s.editor_id,
            s.soul_capacity,
            s.current_soul,
        );
    }
}

/// FO4: vanilla `Fallout4.esm` parse-rate harness. Mirrors the FNV /
/// FO3 patterns. Floors sit a few percent below 2026-05-04 observed
/// counts to absorb patch drift without masking dispatch regressions.
///
/// Closes #819 / FO4-D4-NEW-07 — was missing while FNV / FO3 / Oblivion
/// each had one. Floors specifically lock in the 5 FO4-architecture
/// categories that #817 added to `EsmIndex::categories()`
/// (texture_sets / scols / packins / movables / material_swaps).
#[test]
#[ignore]
fn parse_rate_fo4_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    ) else {
        eprintln!("[FO4] skipping: BYROREDUX_FO4_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("Fallout4.esm");
    let bytes = std::fs::read(&esm_path).expect("read Fallout4.esm");
    let parse_start = std::time::Instant::now();
    let index = parse_esm(&bytes).expect("parse Fallout4.esm");
    let parse_elapsed = parse_start.elapsed();
    eprintln!("[FO4] parse_esm wall={:?}", parse_elapsed);

    let scol_placements: usize = index
        .cells
        .scols
        .values()
        .map(|s| s.parts.iter().map(|p| p.placements.len()).sum::<usize>())
        .sum();

    eprintln!(
        "[FO4] total={} game={:?} | cells={} statics={} scols={} \
         (placements={}) packins={} movables={} material_swaps={} \
         texture_sets={} items={} containers={} LVLI={} LVLN={} NPCs={} \
         races={} classes={} factions={} globals={} game_settings={} \
         weathers={} climates={} trees={}",
        index.total(),
        index.game,
        index.cells.cells.len(),
        index.cells.statics.len(),
        index.cells.scols.len(),
        scol_placements,
        index.cells.packins.len(),
        index.cells.movables.len(),
        index.cells.material_swaps.len(),
        index.cells.texture_sets.len(),
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.npcs.len(),
        index.races.len(),
        index.classes.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
        index.weathers.len(),
        index.climates.len(),
        index.trees.len(),
    );

    // HEDR → GameKind dispatch. Pre-#439 the FO4 master would
    // misclassify as Fallout3NV; this guard keeps that fixed.
    assert_eq!(
        index.game,
        GameKind::Fallout4,
        "FO4 ESM classified as {:?}, expected Fallout4",
        index.game,
    );

    // Primary baseline. With #817 categories landed, observed 2026-05-04
    // is 76,468 records.
    assert!(
        index.total() >= FO4_TOTAL_FLOOR,
        "FO4 total {} < baseline {}",
        index.total(),
        FO4_TOTAL_FLOOR,
    );

    // FO4-architecture categories — #817 made these visible to
    // category_breakdown(). A regression that empties any of them
    // (e.g. `parse_scol_group` rewrite that drops the insert) must
    // fail loud here. Live counts: scols=2617, packins=872,
    // material_swaps=2537, texture_sets=379, movables=0 (vanilla).
    assert!(
        index.cells.scols.len() >= 2500,
        "SCOL={} (expected >= 2500; vanilla ships 2617) — \
         dispatch / parse regression",
        index.cells.scols.len(),
    );
    assert!(
        index.cells.packins.len() >= 850,
        "PKIN={} (expected >= 850; vanilla ships 872)",
        index.cells.packins.len(),
    );
    assert!(
        index.cells.material_swaps.len() >= 2400,
        "MSWP={} (expected >= 2400; vanilla ships 2537)",
        index.cells.material_swaps.len(),
    );
    assert!(
        index.cells.texture_sets.len() >= 376,
        "TXST={} (expected >= 376; vanilla ships 379) — \
         DODT + DNAM now parsed (#813/#814); 3 remaining below ceiling \
         are records with no parseable sub-records in vanilla Fallout4.esm",
        index.cells.texture_sets.len(),
    );
    // MOVS: vanilla ships 0; pin to 0 to catch a future spurious
    // population (DLC-only or mod-content additions can lift this
    // floor when those harnesses arrive).
    assert_eq!(
        index.cells.movables.len(),
        0,
        "MOVS={} (vanilla Fallout4.esm ships 0; non-zero indicates \
         a DLC was loaded — bump the floor when that's expected)",
        index.cells.movables.len(),
    );

    // SCOL placement decode regression guard (#405). 2617 SCOL
    // records expand to 40,330 ONAM-anchored placements on vanilla.
    // A regression in ScolPlacement::from_bytes that returns None
    // unconditionally would drop placement count to 0 while
    // record count stays at 2617.
    assert!(
        scol_placements >= 38_000,
        "SCOL placements = {} (expected >= 38_000; vanilla yields \
         40_330 across 2617 records). #405 ONAM/DATA decode \
         regression suspected.",
        scol_placements,
    );

    // Cell + STAT floors — the FO4 cell loader pipeline depends
    // on these populating before SCOL placements can resolve.
    assert!(
        index.cells.cells.len() >= 900,
        "FO4 cells={} (expected >= 900; vanilla ships 964)",
        index.cells.cells.len(),
    );
    assert!(
        index.cells.statics.len() >= 30_000,
        "FO4 statics={} (expected >= 30_000; vanilla ships 31_989)",
        index.cells.statics.len(),
    );

    // Per-category floors mirroring the FNV / FO3 harness shape.
    // Observed vanilla: items=4076, containers=471, LVLI=2098,
    // LVLN=228, NPCs=3015, factions=699, globals=1346,
    // game_settings=2039, weathers=71, climates=7, races=45,
    // classes=31.
    assert!(index.items.len() >= 3800, "items={}", index.items.len());
    assert!(
        index.containers.len() >= 450,
        "containers={}",
        index.containers.len(),
    );
    assert!(
        index.leveled_items.len() >= 1900,
        "LVLI={}",
        index.leveled_items.len(),
    );
    assert!(
        index.leveled_npcs.len() >= 200,
        "LVLN={}",
        index.leveled_npcs.len(),
    );
    assert!(index.npcs.len() >= 2800, "NPCs={}", index.npcs.len());
    assert!(index.races.len() >= 40, "races={}", index.races.len());
    assert!(index.classes.len() >= 25, "classes={}", index.classes.len(),);
    assert!(
        index.factions.len() >= 660,
        "factions={}",
        index.factions.len(),
    );
    assert!(
        index.globals.len() >= 1200,
        "globals={}",
        index.globals.len(),
    );
    assert!(
        index.game_settings.len() >= 1900,
        "game_settings={}",
        index.game_settings.len(),
    );
    assert!(
        index.weathers.len() >= 60,
        "weathers={}",
        index.weathers.len(),
    );
    assert!(
        index.climates.len() >= 6,
        "climates={}",
        index.climates.len(),
    );
}

/// #967 / OBL-D3-NEW-03 — real-Oblivion RACE coverage. Pins the
/// audit's requested invariant: every vanilla race must surface a
/// non-zero `base_height` (the 1.0 default leaves through DATA
/// short-reads — pre-#967 we never wrote anything to it) AND at
/// least one race must surface non-default voice forms via VNAM.
///
/// `#[ignore]`-gated by Oblivion install (mirrors `parse_rate_oblivion_esm`).
#[test]
#[ignore]
fn race_oblivion_data_and_subs_against_vanilla() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[OBL/RACE] skip: data dir missing");
        return;
    };
    let bytes = std::fs::read(data.join("Oblivion.esm")).expect("read Oblivion.esm");
    let index = parse_esm(&bytes).expect("parse Oblivion.esm");

    assert!(
        index.races.len() >= 15,
        "OBL races={} (vanilla ships at least 15 races)",
        index.races.len(),
    );

    // DATA: every race must surface base_height in the documented
    // 0.5..2.0 range. The 1.0 default is a legitimate authoring
    // value (Imperial / Breton ship 1.0 deliberately), so we can't
    // just check `!= 1.0`. The sanity gate catches NaN / garbage
    // f32 reads without false-negatives on 1.0-author races.
    let sane_heights = index
        .races
        .values()
        .filter(|r| {
            (0.5..=2.0).contains(&r.base_height.0) && (0.5..=2.0).contains(&r.base_height.1)
        })
        .count();
    assert_eq!(
        sane_heights,
        index.races.len(),
        "OBL races with sane base_height={}/{} (NaN / garbage from DATA?)",
        sane_heights,
        index.races.len(),
    );
    // At least one race must author a non-1.0 height — proves the
    // DATA read actually consumed disk bytes and didn't fall through
    // to defaults across the board. Vanilla beast races ship 1.04.
    let non_default_height = index
        .races
        .values()
        .filter(|r| r.base_height.0 != 1.0 || r.base_height.1 != 1.0)
        .count();
    assert!(
        non_default_height >= 5,
        "OBL races with non-default base_height={}/{} \
         (DATA parse never wrote anything?)",
        non_default_height,
        index.races.len(),
    );

    // VNAM / DNAM / ATTR floors — vanilla Oblivion authors these on
    // a SUBSET of races (not every race). Empirical run 2026-05-18:
    // 15 races total / 5 with VNAM / ? with DNAM / ? with ATTR.
    // Each floor is "at least one" so a future regression that
    // silently dropped all of these would fail; the upper bound
    // floats with authoring choices.
    let with_voices = index
        .races
        .values()
        .filter(|r| r.voice_forms.is_some())
        .count();
    assert!(
        with_voices >= 1,
        "OBL races with VNAM voice_forms={}/{} (expected at least 1)",
        with_voices,
        index.races.len(),
    );

    let with_hair = index
        .races
        .values()
        .filter(|r| r.default_hair.is_some())
        .count();
    assert!(
        with_hair >= 1,
        "OBL races with DNAM default_hair={}/{} (expected at least 1)",
        with_hair,
        index.races.len(),
    );

    let with_attr = index
        .races
        .values()
        .filter(|r| r.base_attributes.is_some())
        .count();
    assert!(
        with_attr >= 1,
        "OBL races with ATTR={}/{} (expected at least 1)",
        with_attr,
        index.races.len(),
    );

    eprintln!(
        "[OBL/RACE] races={} | sane_heights={} non_default_heights={} \
         voices={} hairs={} attrs={}",
        index.races.len(),
        sane_heights,
        non_default_height,
        with_voices,
        with_hair,
        with_attr,
    );
}

/// #968 / OBL-D3-NEW-04 — real-Oblivion CLAS coverage. Pins the
/// audit's regression assertion: vanilla "Knight" class must surface
/// `primary_attributes = Some((Strength, Personality))`,
/// `specialization = Some(0 /* Combat */)`, and `major_skills.len() == 7`.
///
/// `#[ignore]`-gated by Oblivion install.
#[test]
#[ignore]
fn clas_oblivion_knight_against_vanilla() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[OBL/CLAS] skip: data dir missing");
        return;
    };
    let bytes = std::fs::read(data.join("Oblivion.esm")).expect("read Oblivion.esm");
    let index = parse_esm(&bytes).expect("parse Oblivion.esm");

    // Vanilla CLAS count: 111 in Oblivion.esm (empirical 2026-05-18).
    assert!(
        index.classes.len() >= 100,
        "OBL classes={} (expected >= 100)",
        index.classes.len(),
    );

    let knight = index
        .classes
        .values()
        .find(|c| c.editor_id == "Knight")
        .expect("vanilla Oblivion.esm must include the 'Knight' CLAS");

    // Strength = 0, Personality = 6 per Oblivion's attribute enum
    // (0..7 = Str/Int/Wil/Agi/Spd/End/Per/Luck).
    assert_eq!(
        knight.primary_attributes,
        Some((0, 6)),
        "Knight.primary_attributes = (Strength, Personality)",
    );
    assert_eq!(
        knight.specialization,
        Some(0),
        "Knight.specialization = 0 (Combat)",
    );
    // Audit asserted major_skills.len() == 7; empirical decode of
    // the 52-byte DATA confirms 7 (vs the audit prose's claim of 14).
    assert_eq!(knight.major_skills.len(), 7);
    // Vanilla majors: Block / Illusion / HeavyArmor / Blunt / Blade /
    // Speechcraft / HandToHand (SkillIndex values).
    assert_eq!(
        knight.major_skills,
        vec![0x0F, 0x17, 0x12, 0x10, 0x0E, 0x20, 0x11],
        "Knight.major_skills = [Block, Illusion, HeavyArmor, Blunt, \
         Blade, Speechcraft, HandToHand]",
    );
    // Playable flag.
    assert_eq!(knight.flags_oblivion, Some(0x01));

    // Sanity gate: every Oblivion class should surface primary
    // attributes + 7 majors. Pre-#968 the FNV arm ran for Oblivion
    // and produced garbage `attribute_weights` + nonsense `tag_skills`.
    let with_primaries = index
        .classes
        .values()
        .filter(|c| c.primary_attributes.is_some() && c.major_skills.len() == 7)
        .count();
    assert_eq!(
        with_primaries,
        index.classes.len(),
        "OBL CLAS with primary_attributes + 7 majors = {}/{} \
         (DATA parse failed on some?)",
        with_primaries,
        index.classes.len(),
    );

    eprintln!(
        "[OBL/CLAS] classes={} | Knight ok | with_primaries={}",
        index.classes.len(),
        with_primaries,
    );
}

// ── #1181 / FO4-D4-004 — unconditional FO4-architecture fixture ─────────
//
// `parse_rate_fo4_esm` above is `#[ignore]`-gated because it needs the
// real Fallout4.esm. CI without game data skips it, so the
// five-map regression net (`texture_sets` / `scols` / `packins` /
// `movables` / `material_swaps` floors in `EsmIndex::categories()`)
// only fires on opt-in. A refactor that silently empties one of those
// maps would not surface in default CI.
//
// This test builds a synthetic Fallout4-shape ESM in-memory — a TES4
// header with HEDR version 1.0 (the FO4 dispatch band per
// `reader.rs::GameKind::from_header`) followed by minimal SCOL / PKIN /
// TXST / MSWP records. After `parse_esm` it asserts each typed map has
// at least one entry. MOVS is omitted because vanilla Fallout4.esm
// ships zero MOVS records — the dispatch arm is exercised by the
// `parse_rate_fo4_esm` ignored-test (which still pins MOVS == 0).
//
// See audit `docs/audits/AUDIT_FO4_2026-05-18.md` D4-004 + #819 (real-
// data harness) + #817 (five-map exposure).

/// Build a 24-byte-header record (`typ`, `form_id`, sub-record list).
/// Mirrors the helper in `crates/plugin/src/esm/records/tests.rs`
/// (private to the unit-test cfg); duplicated here so this integration
/// test stays self-contained.
fn fixture_build_record(typ: &[u8; 4], form_id: u32, subs: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (st, data) in subs {
        sub_data.extend_from_slice(*st);
        sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(data);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(typ);
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC + unknown
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap a record payload in a top-level GRUP (`label`, group_type = 0).
fn fixture_wrap_top_group(label: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let total = 24 + payload.len();
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GRUP");
    buf.extend_from_slice(&(total as u32).to_le_bytes());
    buf.extend_from_slice(label);
    buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC
    buf.extend_from_slice(payload);
    buf
}

/// Build a TES4 file header with HEDR version 1.0 — the FO4 dispatch
/// band per `reader.rs::GameKind::from_header` (`0.98..=1.04` →
/// `Fallout4`).
fn fixture_build_fo4_tes4() -> Vec<u8> {
    let mut hedr = Vec::new();
    hedr.extend_from_slice(b"HEDR");
    hedr.extend_from_slice(&12u16.to_le_bytes());
    hedr.extend_from_slice(&1.0f32.to_le_bytes()); // FO4 version
    hedr.extend_from_slice(&4u32.to_le_bytes()); // record_count (informational)
    hedr.extend_from_slice(&0u32.to_le_bytes()); // next_object_id

    let mut buf = Vec::new();
    buf.extend_from_slice(b"TES4");
    buf.extend_from_slice(&(hedr.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&hedr);
    buf
}

#[test]
fn parse_fo4_architecture_fixture_populates_typed_maps() {
    // SCOL: empty subs still inserts (parse_scol_group has no record-
    // contents condition on the insert). Minimal EDID makes the fixture
    // realistic + survives the StaticObject `editor_id.is_empty()` gate.
    let scol = fixture_build_record(b"SCOL", 0x0010_0001, &[(b"EDID", b"TestScol\0")]);
    let scol_group = fixture_wrap_top_group(b"SCOL", &scol);

    // PKIN: same shape — EDID + the unconditional `packins.insert`.
    let pkin = fixture_build_record(b"PKIN", 0x0020_0001, &[(b"EDID", b"TestPkin\0")]);
    let pkin_group = fixture_wrap_top_group(b"PKIN", &pkin);

    // TXST: needs at least one TX00..TX07 or MNAM so the parsed
    // `TextureSet` differs from `default()` — the walker's insert gate
    // at `cell/support.rs:290` skips records that produce a default-
    // valued set.
    let txst = fixture_build_record(
        b"TXST",
        0x0030_0001,
        &[
            (b"EDID", b"TestTxst\0"),
            (b"TX00", b"textures/test/diffuse.dds\0"),
        ],
    );
    let txst_group = fixture_wrap_top_group(b"TXST", &txst);

    // MSWP: unconditional insert in `parse_mswp_group`.
    let mswp = fixture_build_record(b"MSWP", 0x0040_0001, &[(b"EDID", b"TestMswp\0")]);
    let mswp_group = fixture_wrap_top_group(b"MSWP", &mswp);

    // Assemble the synthetic ESM: TES4 header + the four top-level
    // GRUPs in any order. Walker dispatches by GRUP label so order is
    // free — pick the same as vanilla (TXST → SCOL → PKIN → MSWP) for
    // readability.
    let mut esm = fixture_build_fo4_tes4();
    esm.extend_from_slice(&txst_group);
    esm.extend_from_slice(&scol_group);
    esm.extend_from_slice(&pkin_group);
    esm.extend_from_slice(&mswp_group);

    let index = parse_esm(&esm).expect("parse synthetic FO4 fixture");

    // HEDR → GameKind: 1.0 falls in the (0.98..=1.04) FO4 band.
    assert_eq!(
        index.game,
        GameKind::Fallout4,
        "synthetic HEDR=1.0 must classify as Fallout4 (got {:?})",
        index.game,
    );

    // Five-map regression net floors (the actual #1181 contract).
    // MOVS is intentionally omitted — vanilla ships zero MOVS records;
    // the dispatch arm coverage lives in `parse_rate_fo4_esm`'s
    // `assert_eq!(... movables.len(), 0)` pin.
    assert!(
        !index.cells.scols.is_empty(),
        "SCOL dispatch arm dropped — `scols` map empty after parsing a \
         synthetic SCOL record (#1181 / FO4-D4-004 net)",
    );
    assert!(
        !index.cells.packins.is_empty(),
        "PKIN dispatch arm dropped — `packins` map empty after parsing a \
         synthetic PKIN record (#1181 / FO4-D4-004 net)",
    );
    assert!(
        !index.cells.texture_sets.is_empty(),
        "TXST dispatch arm dropped — `texture_sets` map empty after parsing \
         a synthetic TXST record with TX00 populated (#1181 / FO4-D4-004 net)",
    );
    assert!(
        !index.cells.material_swaps.is_empty(),
        "MSWP dispatch arm dropped — `material_swaps` map empty after parsing \
         a synthetic MSWP record (#1181 / FO4-D4-004 net)",
    );

    // Spot-check the form-IDs landed at the right keys — guards against
    // a future refactor that inserts everything under the wrong map
    // (e.g. SCOL → packins) which would still satisfy the non-empty
    // floors above.
    assert!(
        index.cells.scols.contains_key(&0x0010_0001),
        "SCOL form-id 0x00100001 not present in scols map",
    );
    assert!(
        index.cells.packins.contains_key(&0x0020_0001),
        "PKIN form-id 0x00200001 not present in packins map",
    );
    assert!(
        index.cells.texture_sets.contains_key(&0x0030_0001),
        "TXST form-id 0x00300001 not present in texture_sets map",
    );
    assert!(
        index.cells.material_swaps.contains_key(&0x0040_0001),
        "MSWP form-id 0x00400001 not present in material_swaps map",
    );
}

/// One-off diagnostic for the misplaced-saloon-wall investigation
/// (2026-05-26). Walks `GSProspectorSaloonInterior` REFRs against
/// FalloutNV.esm and emits a TSV-ish dump sorted by spatial
/// position so we can correlate to the in-game render.
///
/// Columns: refr_form, base_form, base_mesh, pos_x, pos_y, pos_z,
///          rot_x_deg, rot_y_deg, rot_z_deg, scale
///
/// Run: `BYROREDUX_FNV_DATA=... cargo test -p byroredux-plugin
///       --release --test parse_real_esm -- --ignored
///       dump_prospector_saloon_refrs --nocapture`
#[test]
#[ignore]
fn dump_prospector_saloon_refrs() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[dump] skipping: BYROREDUX_FNV_DATA unset and fallback path missing");
        return;
    };
    let bytes = std::fs::read(data.join("FalloutNV.esm")).expect("read FalloutNV.esm");
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");

    let key = "gsprospectorsaloonInterior".to_ascii_lowercase();
    let Some(cell) = index.cells.cells.get(&key) else {
        eprintln!(
            "[dump] cell '{key}' not found; got {} interior cells",
            index.cells.cells.len()
        );
        return;
    };

    let mut rows: Vec<_> = cell
        .references
        .iter()
        .map(|r| {
            let mesh = index
                .cells
                .statics
                .get(&r.base_form_id)
                .map(|s| s.model_path.clone())
                .unwrap_or_else(|| String::from("<no base>"));
            (r, mesh)
        })
        .collect();
    // Sort by (mesh-name asc, then position-X) so duplicate base meshes group together.
    rows.sort_by(|a, b| {
        let m = a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase());
        if m == std::cmp::Ordering::Equal {
            a.0.position[0]
                .partial_cmp(&b.0.position[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            m
        }
    });

    eprintln!(
        "[dump] GSProspectorSaloonInterior REFRs: {}\n\
         refr_form\tbase_form\tpos_x\tpos_y\tpos_z\trx_deg\try_deg\trz_deg\tscale\tmesh",
        rows.len()
    );
    let rad2deg = 180.0 / std::f32::consts::PI;
    for (r, mesh) in &rows {
        eprintln!(
            "{:08X}\t{:08X}\t{:>8.1}\t{:>8.1}\t{:>8.1}\t{:>+7.1}\t{:>+7.1}\t{:>+7.1}\t{:.2}\t{}",
            r.form_id,
            r.base_form_id,
            r.position[0],
            r.position[1],
            r.position[2],
            r.rotation[0] * rad2deg,
            r.rotation[1] * rad2deg,
            r.rotation[2] * rad2deg,
            r.scale,
            mesh
        );
    }

    // Tally multi-axis REFRs (those whose rotation has TWO or more
    // non-trivial Euler components — these are the ones that would
    // expose XYZ vs ZYX product divergence post the 2026-05-26 fix).
    let mut multi_axis = 0usize;
    let mut only_z = 0usize;
    let eps = 0.01_f32.to_radians();
    for (r, _) in &rows {
        let nx = r.rotation[0].abs() > eps;
        let ny = r.rotation[1].abs() > eps;
        let nz = r.rotation[2].abs() > eps;
        match (nx, ny, nz) {
            (false, false, _) => only_z += 1,
            (true, _, _) | (_, true, _) => multi_axis += 1,
        }
    }
    eprintln!(
        "[dump] rotation profile: {} multi-axis (rx or ry non-zero), {} z-only / identity",
        multi_axis, only_z
    );

    // Regression assertions (#1320 / TH6-NEW-01): this was a print-only
    // diagnostic that passed vacuously. Pin the invariants that must hold for
    // any valid FNV parse — a populated interior that resolved by key has
    // REFRs, and at least one must link to a base mesh (the base-form join is
    // what the dump's mesh column exercises). Exact counts / rotation-profile
    // bands are intentionally left as printed diagnostics: they need a
    // measured baseline, not a guessed literal.
    assert!(
        !rows.is_empty(),
        "GSProspectorSaloonInterior resolved but produced zero REFRs — parse regression"
    );
    assert!(
        rows.iter().any(|(_, mesh)| mesh != "<no base>"),
        "no REFR in GSProspectorSaloonInterior resolved to a base mesh — \
         base-form linkage regression"
    );
}
