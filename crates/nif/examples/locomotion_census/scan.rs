//! Reading one game's vanilla animation archives.

use super::common::{Game, MeshArchive, game_data_dir, open_present_archive};
use super::roles::{self, Role};
use byroredux_hkx::{HkxAnimation, HkxSkeleton, decode_animation, decode_skeleton};
use std::collections::{BTreeMap, BTreeSet};

/// Horizontal travel below this, in game units over a whole clip, is no displacement.
const MIN_TRAVEL: f32 = 1.0;

/// Name parts that would mark a clip made for a slope or a stair.
const GRADE_TERMS: [&str; 5] = ["slope", "uphill", "downhill", "incline", "stair"];
/// Name parts of a climbing, vaulting or mantling clip.
const CLIMB_TERMS: [&str; 7] = [
    "climb",
    "vault",
    "mantle",
    "ladder",
    "traversal",
    "overwall",
    "upwall",
];
/// Words that contain a grade term and name something else.
const NOT_GRADE: [&str; 1] = ["alistair"];
/// Fallout's Vaults: places, their furniture, their suits and their mascot, none of them a jump.
const NOT_CLIMB: [&str; 10] = [
    "vaulttec",
    "vaultsuit",
    "vaultboy",
    "vaultconsole",
    "vaulttable",
    "trailervault",
    "vault76",
    "vault94",
    "vault96",
    "vault111",
];

/// Whether `path` holds one of `terms` once the words in `not` are struck out.
fn names_any(path: &str, terms: &[&str], not: &[&str]) -> bool {
    // A separator, not nothing, replaces a struck word, so its neighbours cannot join into a term.
    let rest = not
        .iter()
        .fold(path.to_owned(), |text, word| text.replace(word, "|"));
    terms.iter().any(|term| rest.contains(term))
}

/// The vanilla archives that hold a game's actor clips: base game, patch and DLC.
pub fn clip_archives(game: Game) -> Vec<&'static str> {
    match game {
        // The KF games keep clips beside their meshes, so the shared vanilla mesh lists serve.
        Game::Oblivion => game
            .mesh_archives()
            .iter()
            .copied()
            .chain(game.optional_mesh_archives().iter().copied())
            .collect(),
        Game::Fallout3 | Game::FalloutNV => game.mesh_archives().to_vec(),
        Game::SkyrimLE | Game::SkyrimSE => vec!["Skyrim - Animations.bsa"],
        Game::Fallout4 => vec![
            "Fallout4 - Animations.ba2",
            "DLCRobot - Main.ba2",
            "DLCworkshop01 - Main.ba2",
            "DLCCoast - Main.ba2",
            "DLCworkshop02 - Main.ba2",
            "DLCworkshop03 - Main.ba2",
            "DLCNukaWorld - Main.ba2",
        ],
        Game::Fallout76 => vec!["SeventySix - Animations.ba2"],
        Game::Starfield => vec!["Starfield - Animations.ba2", "ShatteredSpace - Main01.ba2"],
    }
}

fn clip_extension(game: Game) -> &'static str {
    match game {
        Game::Oblivion | Game::Fallout3 | Game::FalloutNV => ".kf",
        Game::SkyrimLE | Game::SkyrimSE | Game::Fallout4 | Game::Fallout76 => ".hkx",
        Game::Starfield => ".af",
    }
}

/// Whether the path is an actor's animation clip (not a mesh, skeleton or behaviour file).
fn is_actor_clip(game: Game, path: &str) -> bool {
    if !path.ends_with(clip_extension(game)) {
        return false;
    }
    match clip_extension(game) {
        ".kf" => path.starts_with("meshes/characters/") || path.starts_with("meshes/creatures/"),
        ".hkx" => path.starts_with("meshes/actors/") && path.contains("/animations/"),
        _ => path.starts_with("meshes/actors/"),
    }
}

fn is_humanoid_clip(path: &str) -> bool {
    path.starts_with("meshes/characters/")
        || path.starts_with("meshes/actors/character/")
        || path.starts_with("meshes/actors/human/")
}

/// Where a clip keeps the distance it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Displacement {
    /// A KF accumulation root's translation keys.
    AccumulationRoot,
    /// A Havok animation's extracted motion.
    ExtractedMotion,
    /// A Havok clip's root or centre-of-mass bone track.
    BoneTrack,
    /// No horizontal travel in anything the engine reads.
    NoneRead,
}

impl Displacement {
    pub fn label(self) -> &'static str {
        match self {
            Displacement::AccumulationRoot => "accumulation root",
            Displacement::ExtractedMotion => "extracted motion",
            Displacement::BoneTrack => "root or COM track",
            Displacement::NoneRead => "none read",
        }
    }
}

/// What the census keeps about one readable clip.
#[derive(Debug, Clone)]
pub struct ClipData {
    /// Footfall and sync labels present, sorted and without repeats.
    pub foot_labels: Vec<String>,
    /// Every other label, digits replaced by `N`.
    pub other_labels: BTreeSet<String>,
    pub displacement: Displacement,
    /// Travel over duration, rounded, in game units a second.
    pub speed: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct BaseClip {
    pub name: String,
    pub role: Role,
    /// `Err` holds why the clip was not read.
    pub data: Result<ClipData, String>,
}

#[derive(Debug, Default, Clone)]
pub struct ReadCount {
    pub total: usize,
    pub read: usize,
}

#[derive(Debug)]
pub struct GameCensus {
    pub game: Game,
    pub archives: Vec<&'static str>,
    /// Whether a Havok game's humanoid skeleton decoded, which names the COM track; `None` for the
    /// games that need no skeleton to find their displacement.
    pub skeleton: Option<bool>,
    pub base: Vec<BaseClip>,
    /// Base-set files whose names give no locomotion role.
    pub base_unroled: usize,
    pub humanoid: ReadCount,
    pub other_actors: ReadCount,
    /// Why clips were refused, with counts.
    pub refusals: BTreeMap<String, usize>,
    pub grade_names: Vec<String>,
    /// Directory → clip names that match a climbing term.
    pub climb_names: BTreeMap<String, BTreeSet<String>>,
}

/// Labels that name a footfall.
const FOOTFALL_LABELS: [&str; 8] = [
    "Enum: Left",
    "Enum: Right",
    "FootLeft",
    "FootRight",
    "FootSprintLeft",
    "FootSprintRight",
    "FootFront",
    "FootBack",
];
/// Labels that align one clip's step phase with another's in a blend; not footfalls by name.
const SYNC_LABELS: [&str; 2] = ["SyncLeft", "SyncRight"];

pub fn is_footfall(label: &str) -> bool {
    FOOTFALL_LABELS.contains(&label)
}

/// Split raw labels into foot events and the families of everything else.
pub fn sort_labels<'a>(raw: impl Iterator<Item = &'a str>) -> (Vec<String>, BTreeSet<String>) {
    let mut feet = BTreeSet::new();
    let mut other = BTreeSet::new();
    // One KF text key may hold several lines.
    for label in raw
        .flat_map(|text| text.split(['\r', '\n']))
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        match FOOTFALL_LABELS
            .iter()
            .chain(&SYNC_LABELS)
            .find(|known| known.eq_ignore_ascii_case(label))
        {
            Some(known) => {
                feet.insert((*known).to_owned());
            }
            None => {
                let mut family = String::with_capacity(label.len());
                for c in label.chars() {
                    if !c.is_ascii_digit() {
                        family.push(c);
                    } else if !family.ends_with('N') {
                        family.push('N');
                    }
                }
                other.insert(family);
            }
        }
    }
    (feet.into_iter().collect(), other)
}

fn speed_of(travel: f32, duration: f32) -> Option<u32> {
    (travel >= MIN_TRAVEL && duration > 0.0).then(|| (travel / duration).round() as u32)
}

fn read_kf(bytes: &[u8]) -> Result<ClipData, String> {
    let scene = byroredux_nif::parse_nif(bytes).map_err(|error| error.to_string())?;
    let clips = byroredux_nif::anim::import_kf(&scene);
    let clip = clips.first().ok_or("no sequence in the file")?;
    let (foot_labels, other_labels) =
        sort_labels(clip.text_keys.iter().map(|(_, text)| text.as_str()));
    // Imported keys are in engine space, Y up: the ground plane is x and z.
    let travel = clip
        .accum_root_name
        .as_deref()
        .and_then(|root| {
            clip.channels
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(root))
        })
        .and_then(|(_, channel)| {
            let (first, last) = (
                channel.translation_keys.first()?,
                channel.translation_keys.last()?,
            );
            Some((last.value.x - first.value.x).hypot(last.value.z - first.value.z))
        })
        .unwrap_or(0.0);
    let speed = speed_of(travel, clip.duration);
    Ok(ClipData {
        foot_labels,
        other_labels,
        displacement: if travel >= MIN_TRAVEL {
            Displacement::AccumulationRoot
        } else {
            Displacement::NoneRead
        },
        speed,
    })
}

/// Havok clips are Z up: the ground plane is x and y.
fn havok_travel(clip: &HkxAnimation, skeleton: Option<&HkxSkeleton>) -> (Displacement, f32) {
    if let Some(frame) = &clip.reference_frame
        && let (Some(first), Some(last)) = (frame.samples.first(), frame.samples.last())
    {
        let travel = (last[0] - first[0]).hypot(last[1] - first[1]);
        if travel >= MIN_TRAVEL {
            return (Displacement::ExtractedMotion, travel);
        }
    }
    let travel = clip
        .tracks
        .iter()
        .zip(&clip.track_to_bone)
        .filter(|&(_, &bone)| {
            let com = skeleton
                .and_then(|s| s.bones.get(usize::from(bone)))
                .is_some_and(|b| matches!(b.name.as_str(), "NPC COM [COM ]" | "COM"));
            bone == 0 || com
        })
        .filter_map(|(samples, _)| {
            let (first, last) = (samples.first()?.translation, samples.last()?.translation);
            Some((last[0] - first[0]).hypot(last[1] - first[1]))
        })
        .fold(0.0, f32::max);
    if travel >= MIN_TRAVEL {
        (Displacement::BoneTrack, travel)
    } else {
        (Displacement::NoneRead, 0.0)
    }
}

fn read_hkx(bytes: &[u8], skeleton: Option<&HkxSkeleton>) -> Result<ClipData, String> {
    let clip = decode_animation(bytes).map_err(|error| error.to_string())?;
    let (foot_labels, other_labels) = sort_labels(clip.annotations.iter().map(|a| a.text.as_str()));
    let (displacement, travel) = havok_travel(&clip, skeleton);
    Ok(ClipData {
        foot_labels,
        other_labels,
        displacement,
        speed: speed_of(travel, clip.duration),
    })
}

fn read_clip(game: Game, bytes: &[u8], skeleton: Option<&HkxSkeleton>) -> Result<ClipData, String> {
    match clip_extension(game) {
        ".kf" => read_kf(bytes),
        ".hkx" => read_hkx(bytes, skeleton),
        _ => Err("no reader for `.af`".to_owned()),
    }
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// Scan one game; `None` when its data directory or one of its archives is absent.
pub fn scan(game: Game) -> Option<GameCensus> {
    let data = game_data_dir(game)?;
    let names = clip_archives(game);
    let mut archives: Vec<MeshArchive> = Vec::new();
    for name in &names {
        let path = data.join(name);
        if !path.is_file() {
            eprintln!("{}: {name} not found", game.label());
            return None;
        }
        archives.push(open_present_archive(
            game.label(),
            &path,
            game.archive_kind(),
        ));
    }
    // Normalised path → (archive, stored path). A later archive overrides an earlier one.
    let mut files: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (index, archive) in archives.iter().enumerate() {
        for stored in archive.list_files() {
            files.insert(
                stored.to_ascii_lowercase().replace('\\', "/"),
                (index, stored),
            );
        }
    }
    let extract = |at: &(usize, String)| {
        archives[at.0]
            .extract(&at.1)
            .map_err(|error| error.to_string())
    };
    let skeleton = files
        .iter()
        .find(|(path, _)| {
            path.starts_with("meshes/actors/character/character")
                && path.ends_with("/skeleton.hkx")
                && path.matches('/').count() == 4
        })
        .and_then(|(_, at)| extract(at).ok())
        .and_then(|bytes| decode_skeleton(&bytes).ok());

    let mut census = GameCensus {
        game,
        archives: names,
        skeleton: (clip_extension(game) == ".hkx").then_some(skeleton.is_some()),
        base: Vec::new(),
        base_unroled: 0,
        humanoid: ReadCount::default(),
        other_actors: ReadCount::default(),
        refusals: BTreeMap::new(),
        grade_names: Vec::new(),
        climb_names: BTreeMap::new(),
    };
    for (path, at) in files.iter().filter(|(path, _)| is_actor_clip(game, path)) {
        let result = extract(at).and_then(|bytes| read_clip(game, &bytes, skeleton.as_ref()));
        let count = if is_humanoid_clip(path) {
            &mut census.humanoid
        } else {
            &mut census.other_actors
        };
        count.total += 1;
        match &result {
            Ok(_) => count.read += 1,
            Err(error) => *census.refusals.entry(error.clone()).or_default() += 1,
        }
        let stem = file_stem(path);
        // The directory below the actors root counts too: `ambush/crawlupwall/ambush.hkx`.
        let tail = path.splitn(3, '/').nth(2).unwrap_or(path);
        if names_any(tail, &GRADE_TERMS, &NOT_GRADE) {
            census.grade_names.push(path.clone());
        }
        if names_any(tail, &CLIMB_TERMS, &NOT_CLIMB) {
            let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
            census
                .climb_names
                .entry(directory.to_owned())
                .or_default()
                .insert(stem.to_owned());
        }
        if roles::in_base_set(game, path) {
            match roles::role_of(stem) {
                Some(role) => census.base.push(BaseClip {
                    name: stem.to_owned(),
                    role,
                    data: result,
                }),
                None => census.base_unroled += 1,
            }
        }
    }
    census
        .base
        .sort_by(|a, b| (a.role, &a.name).cmp(&(b.role, &b.name)));
    Some(census)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_split_by_line_and_foot_events_are_matched_without_case() {
        let (feet, other) = sort_labels(
            [
                "Enum: Left\r\nEnum: FoleyWalk",
                "footright",
                "Blend: 12",
                "Blend:6",
            ]
            .into_iter(),
        );
        assert_eq!(feet, ["Enum: Left", "FootRight"]);
        assert_eq!(
            other.into_iter().collect::<Vec<_>>(),
            ["Blend: N", "Blend:N", "Enum: FoleyWalk"]
        );
    }

    #[test]
    fn a_vault_or_an_alistair_is_not_a_climb_or_a_stair() {
        for path in [
            "character/animations/furniture/76trailervaultexit01/posea_idle1.hkx",
            "emotes/vault94.hkx",
        ] {
            assert!(!names_any(path, &CLIMB_TERMS, &NOT_CLIMB), "{path}");
        }
        for path in [
            "rifleidlereadykneelrightvault64.hkx",
            "feralghoul/animations/ambush/crawlupwall/ambush.hkx",
        ] {
            assert!(names_any(path, &CLIMB_TERMS, &NOT_CLIMB), "{path}");
        }
        assert!(!names_any(
            "emotes/alistair_triumphant.hkx",
            &GRADE_TERMS,
            &NOT_GRADE
        ));
        assert!(names_any("walkupstairs.kf", &GRADE_TERMS, &NOT_GRADE));
    }

    #[test]
    fn only_travel_with_a_duration_gives_a_speed() {
        assert_eq!(speed_of(101.6, 1.2), Some(85));
        assert_eq!(speed_of(0.5, 1.2), None);
        assert_eq!(speed_of(100.0, 0.0), None);
    }
}
