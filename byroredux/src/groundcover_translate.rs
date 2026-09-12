//! EXAL ground cover — the translate boundary (Phase 0).
//!
//! Per-game vegetation data enters the engine at exactly this file and nowhere
//! else, per the format-translation doctrine: nothing downstream of here — no
//! shader, no scatter pass, no LOD tier — ever branches on which game supplied
//! the palette, or on whether one did at all.
//!
//! Two things are resolved here:
//!
//! 1. **Layer affinity** — a landscape-texture name maps to a `cover_affinity`
//!    scalar. This is the key reframing of design §3: a splat layer does not
//!    *enable* ground cover, it *weights* it. A dirt layer at 0.20 and a grass
//!    layer at 0.95 blend into a continuous gradient wherever the level artist
//!    feathered them, so the vegetation boundary stops coinciding with the
//!    texture boundary — which is the single biggest cause of vanilla grass
//!    reading as patches.
//! 2. **Palette + wind** — the species set and canonical [`WindField`] for a
//!    worldspace.
//!
//! # The keyword table is grounded, not invented
//!
//! The keywords below were derived from the actual `LTEX` corpus of the four
//! installed games — 386 unique records (Oblivion 229, FNV 89, Skyrim 68, FO3
//! 51) — by tokenising every editor ID and ranking by frequency. `terrain`
//! (151), `grass` (131), `dirt` (80), `moss` (54), `rock` (46) and so on are
//! measured, not guessed.
//!
//! The most important thing that corpus revealed is [`SUPPRESSION_KEYWORDS`]:
//! **46 records carry an explicit `NoGrass` suffix** — `CHTerrainGrass01NoGrass`,
//! `LTundra01NoGrass`, `DementiaMoss01NoGrass`, `LGrassGreenSuburbsNoGrass`.
//! These are authored variants of a grass texture with vegetation deliberately
//! suppressed, used for paths worn through a meadow and for ground under
//! buildings. A naive `contains("grass")` test scores them *highest* when they
//! mean the exact opposite, so suppression is checked first and wins outright.
//! That single ordering rule is the difference between grass respecting worn
//! paths and grass growing through them.
//!
//! # Calibration status
//!
//! The affinity *values* are initial estimates. Design §11.3 is explicit that
//! the density field's numbers need tuning against real cells, pinned with a
//! density histogram over a fixed camera path per game rather than a unit test
//! — because the formula itself lives in GLSL. The tests here therefore pin
//! *ordering and structure* (grass outranks dirt outranks rock; suppression
//! beats every positive match), never the exact scalars, so calibration can
//! move the numbers without rewriting the suite.

use byroredux_core::ecs::components::groundcover::{
    Climate, ClimateWeights, GroundCoverPalette, GroundCoverSpecies, WindField,
};
use byroredux_plugin::esm::records::GrasRecord;
use std::collections::HashMap;

/// Affinity for a layer whose name matches no keyword.
///
/// Deliberately low-but-nonzero. Zero would make an unrecognised layer a hard
/// vegetation hole — reintroducing the boolean-boundary artifact the whole
/// design exists to remove — while a high default would carpet asphalt in any
/// game whose naming we have not seen.
///
/// # Consumer status (#4054 updated this from the original Phase-0 note)
///
/// `layer_affinity` (singular) now has a real, non-test production consumer:
/// `cell_loader/terrain.rs`'s `CellSplatLayer` builder, which flows into
/// `render/groundcover.rs`'s GPU upload. Only the GPU-side
/// `groundcover_scatter.comp` `affinity(splat)` dispatch itself is still
/// pending (Phase 1) — the CPU-side resolution this module does is
/// already live. The sibling `layer_affinities` (plural, batch form) is
/// still genuinely uncalled outside its own test and correctly keeps its
/// `#[allow(dead_code)]`. The palette/wind half below is already live via
/// `install_ground_cover`.
///
/// #4054 — re-exported from `byroredux_core` rather than defined here: the
/// scatter shader needs the same number for unpainted ground, and it reaches
/// GLSL through the generated header.
pub use byroredux_core::ecs::components::groundcover::DEFAULT_COVER_AFFINITY as DEFAULT_AFFINITY;

/// Substrings that mean "vegetation is deliberately absent here", checked
/// before every positive keyword and winning outright.
///
/// `nograss` is the authored Bethesda convention (46 records across the
/// corpus). The others catch hard surfaces whose names also contain a positive
/// token — `LScrubAsphaltStripGRASS` is asphalt with a grass verge in the
/// texture, not a lawn.
const SUPPRESSION_KEYWORDS: &[&str] = &["nograss", "nonegrass", "lava", "asphalt", "pavement"];

/// Positive substrate keywords, **longest-first within equal specificity** so a
/// more specific token cannot be shadowed by a shorter one it contains.
///
/// Ordering is load-bearing: `cobblestone` must precede `stone`, and
/// `rockymoss` must precede both `rocky` and `moss`, or the coarser token wins
/// and the finer distinction is lost.
const AFFINITY_KEYWORDS: &[(&str, f32)] = &[
    // ── strongly vegetated ──────────────────────────────
    // `grass` sits near the top on purpose: several corpus names combine it
    // with an otherwise-bare token (`RootsBarrenWastesGrass01`,
    // `ChemicalBarrenWastes01Grass`) and the vegetated reading is correct
    // there — the artist painted grass over the barren base.
    ("rockymoss", 0.45),
    ("grassdirt", 0.75),
    ("dirtgrass", 0.75),
    ("grass", 0.95),
    ("meadow", 0.85),
    ("moss", 0.55),
    ("tundra", 0.50),
    ("scrub", 0.45),
    ("marsh", 0.45),
    ("lichen", 0.40),
    ("sage", 0.40),
    ("fungus", 0.35),
    ("vines", 0.35),
    ("forest", 0.35),
    ("clover", 0.85),
    ("mold", 0.30),
    ("roots", 0.30),
    ("leaves", 0.30),
    ("litter", 0.30),
    // Conifer needle drop is mildly hostile ground — acidic and matted — so it
    // sits below broadleaf litter rather than beside it.
    ("needles", 0.20),
    ("tilledsoil", 0.30),
    ("soil", 0.30),
    // ── worn / trodden ──────────────────────────────────
    // Ahead of their substrate tokens: `LDirtPathWasteland01` is a path worn
    // through dirt, and the path is why cover is absent. Reading it as plain
    // dirt would grow grass straight across the trail.
    ("street", 0.0),
    ("path", 0.05),
    ("trail", 0.05),
    // ── bare but plausible ──────────────────────────────
    ("riverbottom", 0.15),
    ("riverbed", 0.20),
    ("muck", 0.25),
    ("mudslime", 0.20),
    ("mud", 0.25),
    ("silt", 0.18),
    ("dirt", 0.20),
    ("crackedearth", 0.08),
    ("scorched", 0.03),
    ("burnt", 0.08),
    ("burned", 0.08),
    ("earth", 0.15),
    ("chemical", 0.02),
    ("barren", 0.08),
    ("ash", 0.05),
    ("beach", 0.06),
    ("coast", 0.08),
    ("sand", 0.10),
    ("gravel", 0.08),
    // ── hard surfaces ───────────────────────────────────
    ("cobblestone", 0.02),
    ("rubble", 0.05),
    ("pebbles", 0.10),
    ("rocks", 0.03),
    ("rock", 0.03),
    ("stone", 0.03),
    ("cliff", 0.01),
    ("snow", 0.02),
    ("ice", 0.0),
    ("road", 0.02),
    ("floor", 0.0),
    ("brick", 0.0),
    ("canvas", 0.0),
    ("metal", 0.0),
];

/// Resolve a landscape-texture name to its `cover_affinity` weight.
///
/// `name` may be an editor ID (`LGrassGreenSuburbs`) or a texture path
/// (`Dementia\DementiaMoss01.dds`) — Oblivion supplies the latter via `LTEX`'s
/// `ICON`, every other game the former via `TNAM` → `TXST`. Matching is
/// case-insensitive substring, so both shapes work without a separate path.
pub fn layer_affinity(name: &str) -> f32 {
    let lowered = name.to_ascii_lowercase();
    if SUPPRESSION_KEYWORDS.iter().any(|k| lowered.contains(k)) {
        return 0.0;
    }
    for (keyword, affinity) in AFFINITY_KEYWORDS {
        if lowered.contains(keyword) {
            return *affinity;
        }
    }
    DEFAULT_AFFINITY
}

/// Resolve affinities for a cell's splat layers, in layer order.
///
/// The scatter pass dots this against the bilinearly-sampled splat weights to
/// get the `affinity(splat)` term of §3. Layers with no name resolve to
/// [`DEFAULT_AFFINITY`] rather than zero, for the same reason the default is
/// nonzero.
#[allow(dead_code)] // see DEFAULT_AFFINITY — Phase 1 scatter is the consumer
pub fn layer_affinities(names: &[Option<&str>]) -> Vec<f32> {
    names
        .iter()
        .map(|n| n.map_or(DEFAULT_AFFINITY, layer_affinity))
        .collect()
}

/// Classify a vegetation climate from a worldspace's `WNAM` ancestry, most
/// specific first.
///
/// Child worldspaces routinely carry no geographic signal in their own editor
/// ID while sitting physically inside a parent that does. FO3's `MegatonWorld`
/// is the case that forced this: classified on its own name it reads
/// `Temperate`, when Megaton is a Capital Wasteland settlement and every
/// surrounding cell is arid. Walking up to its parent recovers the right answer.
///
/// The first ancestor that matches wins, so a child's own signal still overrides
/// its parent's — a marsh inside a temperate province stays wetland.
pub fn climate_for_worldspace_chain(chain: &[String]) -> Climate {
    for key in chain {
        if let Some(climate) = classify_worldspace_name(key) {
            return climate;
        }
    }
    Climate::Temperate
}

/// The matcher behind [`climate_for_worldspace_chain`]. `None` means "this
/// name carries no geographic signal", which is what lets the chain walk keep
/// looking at ancestors instead of stopping on a default.
fn classify_worldspace_name(editor_id: &str) -> Option<Climate> {
    let lowered = editor_id.to_ascii_lowercase();
    const ARID: &[&str] = &[
        "wasteland",
        "mojave",
        "capital",
        "desert",
        "barren",
        "dunes",
    ];
    const ALPINE: &[&str] = &["winterhold", "snow", "frozen", "pale", "solstheim"];
    const WETLAND: &[&str] = &["marsh", "swamp", "bog", "hjaalmarch", "blackmarsh"];
    // Wetland before alpine before arid: standing water is the strongest
    // vegetation signal, and `FrozenMarsh` must not read as merely alpine.
    if WETLAND.iter().any(|k| lowered.contains(k)) {
        return Some(Climate::Wetland);
    }
    if ALPINE.iter().any(|k| lowered.contains(k)) {
        return Some(Climate::Alpine);
    }
    if ARID.iter().any(|k| lowered.contains(k)) {
        return Some(Climate::Arid);
    }
    None
}

/// Build the ground-cover palette for a worldspace, over its full `WNAM`
/// ancestry.
///
/// `authored` carries species translated from `GRAS` records (design §7
/// precedence 1) — [`resolve_palette_from_grasses`] is the wiring that
/// produces it from a plugin's records. An empty `authored` is not a stub:
/// content with no vegetation data is meant to fall through to the built-in
/// species, which is why the fallback lives in
/// [`GroundCoverPalette::resolve`] rather than behind a `TODO` here.
pub fn resolve_palette_for_chain(
    chain: &[String],
    authored: Vec<GroundCoverSpecies>,
) -> GroundCoverPalette {
    GroundCoverPalette::resolve(authored, climate_for_worldspace_chain(chain))
}

/// Build a worldspace's palette from a plugin's `GRAS` records — the live
/// Phase 5 entry point (#3807).
///
/// Exists so the ordering constraint lives in exactly one place: the
/// climate has to be resolved *before* the species, because a species'
/// base sizing and colour come from the climate default it is layered
/// over. A caller doing this itself would have to know that.
pub fn resolve_palette_from_grasses(
    chain: &[String],
    grasses: &HashMap<u32, GrasRecord>,
) -> GroundCoverPalette {
    let climate = climate_for_worldspace_chain(chain);
    resolve_palette_for_chain(chain, authored_species(grasses, climate))
}

/// Ceiling on a `GRAS`-derived plant height, in Gamebryo units.
///
/// Ground cover stops at low scrub by design (§10) — trees and shrubs have
/// their own authority — and a `MODB` radius or `OBND` box can be authored
/// to anything. The tallest real vanilla record is Skyrim's 279 units
/// (corpus max over all 138 dimensioned records; median 63), so this
/// rejects nothing authentic while keeping a corrupt or mod-authored
/// record from putting a 40-metre blade in the palette.
const MAX_SPECIES_HEIGHT: f32 = 512.0;

/// Upper clamp on `GRAS.height_range`, which is a ± *fraction* of the
/// model's own height.
///
/// Vanilla data tops out at 0.85. A fraction at or above 1.0 would drive
/// the low end of the range to zero or negative — a blade with no height
/// silently vanishes from the raster with no diagnostic, which is exactly
/// the failure `GroundCoverSpecies::is_well_formed` exists to catch.
const MAX_HEIGHT_VARIATION: f32 = 0.9;

/// Climate keywords for a `GRAS` editor ID, in the same shape — and the
/// same precedence — as [`classify_worldspace_name`].
///
/// Grounded in the real corpus of all 168 vanilla `GRAS` records
/// (Oblivion 108, FO3 9, FNV 24, Skyrim SE 27, census 2026-09-06) by
/// tokenising every editor ID: `wasteland` (22), `sea` (9), `snow` (7),
/// `water` (7), `cattail` (4), `sage` (4), `oasis` (3) are measured
/// frequencies, not invented tokens.
///
/// Wetland is tested before alpine before arid for the same reason the
/// worldspace matcher does it: standing water is the strongest vegetation
/// signal, and Skyrim's `FrozenMarshGrass01` must not read as merely
/// alpine when it names a marsh.
fn classify_species_name(editor_id: &str) -> Option<Climate> {
    let lowered = editor_id.to_ascii_lowercase();
    const WETLAND: &[&str] = &[
        "underwater",
        "cattail",
        "marsh",
        "kelp",
        "coral",
        "oasis",
        "beach",
        "water",
        "sea",
    ];
    const ALPINE: &[&str] = &["frozen", "snow"];
    const ARID: &[&str] = &["wasteland", "volcanic", "desert", "sage"];
    if WETLAND.iter().any(|k| lowered.contains(k)) {
        return Some(Climate::Wetland);
    }
    if ALPINE.iter().any(|k| lowered.contains(k)) {
        return Some(Climate::Alpine);
    }
    if ARID.iter().any(|k| lowered.contains(k)) {
        return Some(Climate::Arid);
    }
    None
}

/// Selection weights for a species whose editor ID resolved to `climate`.
///
/// Generalises the profile already authored on
/// [`GroundCoverSpecies::DEFAULT_ARID`] (0.4 / 2.0 / 0.1 / 0.1) rather
/// than inventing a second shape: strong presence in the matched climate,
/// a reduced showing in temperate as the generic middle ground, and a
/// small but non-zero tail elsewhere. Non-zero matters — a hard zero is
/// the boolean boundary the whole design exists to remove, one level up.
fn climate_weights_for(climate: Climate) -> ClimateWeights {
    const STRONG: f32 = 2.0;
    const MIDDLE: f32 = 0.4;
    const TAIL: f32 = 0.1;
    let pick = |c: Climate| {
        if c == climate {
            STRONG
        } else if c == Climate::Temperate {
            MIDDLE
        } else {
            TAIL
        }
    };
    ClimateWeights {
        temperate: pick(Climate::Temperate),
        arid: pick(Climate::Arid),
        alpine: pick(Climate::Alpine),
        wetland: pick(Climate::Wetland),
    }
}

/// Translate one `GRAS` record into a canonical species (design §7
/// precedence 1), or `None` when it carries nothing the built-in default
/// does not already provide.
///
/// # What crosses the boundary, and what does not
///
/// Only the record's **dimensions** and its editor ID's climate signal —
/// and the dimensions are a *clump's*, read as a ceiling on blade height
/// rather than as a blade measurement; see the height derivation in the
/// body.
/// `density`, `min_slope`/`max_slope`, `distance_from_water` and
/// `position_range` are placement authority, and placement is entirely
/// engine-authored here (§1) — the density field derives its own slope
/// gate from the terrain normal and its own moisture term from the WATAL
/// water plane, which is the point of the design rather than an omission.
///
/// `colour_gradient`, `width_range`, `bend_stiffness` and `cover_affinity`
/// stay at the climate default:
///
/// - **Colour** comes from the model's *texture* (§7), which needs the
///   archive-backed asset provider, not the record. Deferred, and left as
///   the base gradient rather than approximated from a field that does not
///   encode it — `colour_range` is a per-instance jitter *amount*, not a
///   colour.
/// - **Width** has no `GRAS` field at all. The `OBND` x/y extent is the
///   clump's footprint, not a blade width, and reading it as one would
///   make an 80-unit-wide clump into 80-unit-wide blades.
/// - **Stiffness** is not derivable from `wave_period`: its scale is not
///   comparable across games (Oblivion 0.0001–30, Skyrim 50–600) and it
///   does not correlate with plant height in any corpus (Spearman +0.11
///   on Oblivion's n=99, −0.05 on Skyrim's n=21). Mapping it anyway would
///   be inventing a heuristic the data contradicts.
/// - **Affinity** would have to come from `density`, which §1 rejects.
///
/// Returns `None` for a record with no usable dimensions — 9 of Oblivion's
/// 108 (`MODB` 0.0), 15 of FNV's 24 and 6 of Skyrim's 27 (all-zero
/// `OBND`). Such a record would contribute a byte-for-byte copy of the
/// default species, adding palette weight without adding information.
pub fn species_from_gras(record: &GrasRecord, climate: Climate) -> Option<GroundCoverSpecies> {
    let height = record.nominal_height()?;
    if !(height.is_finite() && height > 0.0 && height <= MAX_SPECIES_HEIGHT) {
        return None;
    }
    // `GRAS.height_range` is a ± fraction applied to the *whole model* per
    // placed instance, and the model is a clump. It sets the width of the
    // canonical range — a record at 0.375 spreads wider than one at 0.2,
    // which is what gives a patch natural height variance instead of a
    // uniform lawn — but not its centre; see the height derivation below.
    //
    // Finiteness is checked *before* the clamp, not left to it: `clamp`
    // maps `+inf` to the ceiling, so a corrupt record would come out as
    // "maximum variation" — a guess at intent dressed up as a valid value.
    // (`NaN` does survive `clamp` and is caught by `is_well_formed` below,
    // but relying on two different mechanisms for the same class of bad
    // input is how one of them ends up unreachable and untested.)
    if !record.height_range.is_finite() {
        return None;
    }
    let variation = record.height_range.clamp(0.0, MAX_HEIGHT_VARIATION);
    let base = match climate {
        Climate::Arid => GroundCoverSpecies::DEFAULT_ARID,
        _ => GroundCoverSpecies::DEFAULT_TEMPERATE,
    };
    // The clump height is the range's **ceiling**, not its centre.
    //
    // A vanilla `GRAS` model is a cluster of blades, not one blade:
    // `GrassWasteland06`'s bounds are 66 × 67 × 70 units, a roughly cubic
    // volume that no single blade occupies. So the record's height is
    // approximately its *tallest* blade, and centring a per-blade range on
    // it would make the typical blade as tall as the whole clump it came
    // from — Bethesda-sized grass arriving through the back door of a
    // design whose entire premise (§1) is not to reproduce that look.
    //
    // Taking it as the ceiling states only what the record actually
    // supports: nothing grows taller than the clump it was measured from.
    // The intra-clump distribution — how short the shortest blade in a
    // cluster is — is not in the record, so the authored variation stands
    // in for it rather than being invented alongside it.
    let species = GroundCoverSpecies {
        height_range: (height * (1.0 - variation), height),
        climate_weight: classify_species_name(&record.editor_id)
            .map_or(ClimateWeights::UNIFORM, climate_weights_for),
        ..base
    };
    // A record can still be malformed in ways the checks above miss (a
    // non-finite `height_range` survives `clamp` as NaN). The palette drops
    // malformed species anyway; rejecting here keeps the *count* honest so
    // the load-time diagnostic reports what actually reached the palette.
    species.is_well_formed().then_some(species)
}

/// Translate a plugin's whole `GRAS` set into the authored half of a
/// worldspace palette.
///
/// Every `GRAS` in the load order is a candidate, deliberately: species
/// selection is by climate weight × local conditions (§7), never by which
/// `LTEX` a record was keyed to. Keying off `LTEX` is the Creation Engine
/// model whose hard texture boundaries are the artifact this design exists
/// to remove, so the association is dropped rather than translated.
///
/// Ordered by FormID so a palette is byte-identical across runs — the
/// scatter pass indexes species by position, and a `HashMap` iteration
/// order would make a blade change species between sessions.
pub fn authored_species(
    grasses: &HashMap<u32, GrasRecord>,
    climate: Climate,
) -> Vec<GroundCoverSpecies> {
    let mut by_form: Vec<_> = grasses.iter().collect();
    by_form.sort_unstable_by_key(|(form_id, _)| **form_id);
    by_form
        .into_iter()
        .filter_map(|(_, record)| species_from_gras(record, climate))
        .collect()
}

/// Canonical wind for the current weather when no authored direction is
/// available. The stable worldspace hash keeps legacy/fallback content
/// deterministic across sessions while still giving different worlds
/// different prevailing directions.
pub fn resolve_wind(editor_id: &str, wind_speed: u8) -> WindField {
    resolve_wind_with_direction(editor_id, wind_speed, None)
}

/// Resolve a weather wind, preferring the direction translated from a Skyrim
/// WTHR record and falling back to the stable worldspace direction used by
/// older records. Installation happens before the first `weather_system` tick,
/// so using this at the boundary prevents one frame of grass/water/smoke flow
/// from disagreeing with the authored direction.
pub fn resolve_wind_with_direction(
    editor_id: &str,
    wind_speed: u8,
    authored_direction: Option<[f32; 2]>,
) -> WindField {
    let mut hash: u32 = 2_166_136_261;
    for byte in editor_id.as_bytes() {
        hash ^= u32::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(16_777_619);
    }
    let angle = (hash % 3600) as f32 * (std::f32::consts::TAU / 3600.0);
    let fallback_direction = [angle.cos(), angle.sin()];
    let direction = authored_direction.filter(|candidate| {
        candidate.iter().all(|value| value.is_finite())
            && candidate[0] * candidate[0] + candidate[1] * candidate[1] > 1.0e-8
    });
    WindField::from_weather_byte(wind_speed, direction.unwrap_or(fallback_direction))
}

#[cfg(test)]
#[path = "groundcover_translate_tests.rs"]
mod tests;
