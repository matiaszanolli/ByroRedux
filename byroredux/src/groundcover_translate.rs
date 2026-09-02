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
    Climate, GroundCoverPalette, GroundCoverSpecies, WindField,
};

/// Affinity for a layer whose name matches no keyword.
///
/// Deliberately low-but-nonzero. Zero would make an unrecognised layer a hard
/// vegetation hole — reintroducing the boolean-boundary artifact the whole
/// design exists to remove — while a high default would carpet asphalt in any
/// game whose naming we have not seen.
///
/// # Why this is `dead_code` today
///
/// The affinity half of this module has exactly one consumer by design: the
/// `affinity(splat)` term of the Phase 1 `groundcover_scatter.comp` dispatch,
/// which does not exist yet. Phase 0's job is to land the boundary — the values
/// are fully exercised by the test suite against the real `LTEX` corpus, so
/// this is a *pending* consumer rather than unused code. The palette/wind half
/// below is already live via `install_ground_cover`.
#[allow(dead_code)]
pub const DEFAULT_AFFINITY: f32 = 0.15;

/// Substrings that mean "vegetation is deliberately absent here", checked
/// before every positive keyword and winning outright.
///
/// `nograss` is the authored Bethesda convention (46 records across the
/// corpus). The others catch hard surfaces whose names also contain a positive
/// token — `LScrubAsphaltStripGRASS` is asphalt with a grass verge in the
/// texture, not a lawn.
#[allow(dead_code)] // see DEFAULT_AFFINITY — Phase 1 scatter is the consumer
const SUPPRESSION_KEYWORDS: &[&str] = &["nograss", "nonegrass", "lava", "asphalt", "pavement"];

/// Positive substrate keywords, **longest-first within equal specificity** so a
/// more specific token cannot be shadowed by a shorter one it contains.
///
/// Ordering is load-bearing: `cobblestone` must precede `stone`, and
/// `rockymoss` must precede both `rocky` and `moss`, or the coarser token wins
/// and the finer distinction is lost.
#[allow(dead_code)] // see DEFAULT_AFFINITY — Phase 1 scatter is the consumer
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
#[allow(dead_code)] // see DEFAULT_AFFINITY — Phase 1 scatter is the consumer
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
/// `authored` carries species translated from `GRAS` records when a game
/// supplies them (design §7 precedence 1, landing in Phase 5); today it is
/// empty for every game and the built-in default carries the feature. That is
/// the intended end state for content with no vegetation data, not a stub —
/// which is why the fallback lives in [`GroundCoverPalette::resolve`] rather
/// than behind a `TODO` here.
pub fn resolve_palette_for_chain(
    chain: &[String],
    authored: Vec<GroundCoverSpecies>,
) -> GroundCoverPalette {
    GroundCoverPalette::resolve(authored, climate_for_worldspace_chain(chain))
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
