//! The legacy mod mechanism catalog (`assets/legacy_mod_compat/mechanisms.toml`, PRD legacy-mod-compatibility §3.1,
//! LMC-CLS-006) as the preflight reads it (LMC-DX-001): each mechanism's disposition per game, and the patterns that
//! classify a mod's files, the files beside the game's executable, and the keys of a game-format INI a mod ships.
//!
//! A path takes the mechanism of its most specific matching pattern, the one with the most literal characters (ties go
//! to the earlier entry). A path no pattern matches is unclassified: the preflight reports it, and §7.3 rule 2 fails
//! on it, so the catalog grows by data rather than by a guess in code. Patterns are lowercase and `/`-separated; `*`
//! matches within one path segment and `**` across any number of them (checked by `scripts/check-lmc-catalog.py`
//! C15).

use std::sync::OnceLock;

use serde::Deserialize;

/// The catalog as checked in, embedded so the preflight needs no install-relative path.
const CATALOG: &str = include_str!("../../../assets/legacy_mod_compat/mechanisms.toml");

#[derive(Debug, Deserialize)]
struct RawCatalog {
    catalog_version: u32,
    mechanism: Vec<RawMechanism>,
    #[serde(default)]
    not_game_data: RawPatterns,
}

#[derive(Debug, Default, Deserialize)]
struct RawPatterns {
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawMechanism {
    id: String,
    name: String,
    disposition: String,
    games: Vec<String>,
    #[serde(default)]
    per_game: std::collections::HashMap<String, String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    game_dir_paths: Vec<String>,
    #[serde(default)]
    ini_keys: Vec<String>,
}

/// One compiled pattern: its segments and how many literal characters it has.
#[derive(Debug, Clone)]
struct Pattern {
    segments: Vec<String>,
    literal: usize,
}

impl Pattern {
    fn new(text: &str) -> Self {
        Self {
            segments: text.split('/').map(str::to_owned).collect(),
            literal: text.chars().filter(|c| !matches!(c, '*' | '/')).count(),
        }
    }

    fn matches(&self, path: &[&str]) -> bool {
        match_segments(&self.segments, path)
    }
}

fn match_segments(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => (0..=path.len()).any(|skip| match_segments(rest, &path[skip..])),
        Some((first, rest)) => {
            path.split_first().is_some_and(|(segment, tail)| match_wildcard(first.as_bytes(), segment.as_bytes()) && match_segments(rest, tail))
        }
    }
}

/// `*` matches any run of characters within one segment.
fn match_wildcard(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| match_wildcard(rest, &text[skip..])),
        Some((c, rest)) => text.split_first().is_some_and(|(t, tail)| t == c && match_wildcard(rest, tail)),
    }
}

/// One mechanism of the catalog.
#[derive(Debug, Clone)]
pub struct Mechanism {
    pub id: String,
    pub name: String,
    disposition: String,
    games: Vec<String>,
    per_game: std::collections::HashMap<String, String>,
    paths: Vec<Pattern>,
    game_dir_paths: Vec<Pattern>,
    ini_keys: Vec<Pattern>,
}

impl Mechanism {
    /// The disposition (`D1`..`D5`) the catalog gives this mechanism for `game`.
    pub fn disposition_for(&self, game: &str) -> &str {
        self.per_game.get(game).unwrap_or(&self.disposition)
    }

    /// Whether the mechanism exists for `game`.
    pub fn exists_for(&self, game: &str) -> bool {
        self.games.iter().any(|g| g == game)
    }
}

/// What a path or key is, by the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classified<'a> {
    Mechanism(&'a Mechanism),
    /// Documentation, installer metadata, screenshots: shipped with a mod, read by no game.
    NotGameData,
    Unclassified,
}

impl PartialEq for Mechanism {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Mechanism {}

/// Which pattern list a lookup reads.
#[derive(Clone, Copy)]
enum Field {
    Paths,
    GameDir,
    IniKeys,
}

/// The catalog the preflight classifies by.
#[derive(Debug)]
pub struct ModCatalog {
    pub version: u32,
    mechanisms: Vec<Mechanism>,
    not_game_data: Vec<Pattern>,
}

impl ModCatalog {
    /// The checked-in catalog.
    pub fn embedded() -> &'static ModCatalog {
        static CATALOG_CELL: OnceLock<ModCatalog> = OnceLock::new();
        CATALOG_CELL.get_or_init(|| Self::parse(CATALOG).expect("assets/legacy_mod_compat/mechanisms.toml parses"))
    }

    /// Parse a catalog in the checked-in format.
    pub fn parse(text: &str) -> Result<ModCatalog, toml::de::Error> {
        let raw: RawCatalog = toml::from_str(text)?;
        let compile = |patterns: &[String]| patterns.iter().map(|p| Pattern::new(p)).collect();
        Ok(ModCatalog {
            version: raw.catalog_version,
            mechanisms: raw
                .mechanism
                .into_iter()
                .map(|m| Mechanism {
                    paths: compile(&m.paths),
                    game_dir_paths: compile(&m.game_dir_paths),
                    ini_keys: compile(&m.ini_keys),
                    id: m.id,
                    name: m.name,
                    disposition: m.disposition,
                    games: m.games,
                    per_game: m.per_game,
                })
                .collect(),
            not_game_data: compile(&raw.not_game_data.paths),
        })
    }

    /// The mechanism `id`.
    pub fn get(&self, id: &str) -> Option<&Mechanism> {
        self.mechanisms.iter().find(|m| m.id == id)
    }

    /// The mechanism of a file a mod root ships, by its path relative to the root (any case, either separator), for
    /// `game` (a catalog game: `oblivion`, `fo3`, `fnv`, `skyrim`, `fo4`, `starfield`).
    pub fn classify_path(&self, game: &str, relative: &str) -> Classified<'_> {
        self.classify(game, relative, Field::Paths, true)
    }

    /// The mechanism of a file beside the game's executable. Unmatched files there are the game's own, so the caller
    /// reports only `Mechanism` results.
    pub fn classify_game_dir(&self, game: &str, relative: &str) -> Classified<'_> {
        self.classify(game, relative, Field::GameDir, false)
    }

    /// The mechanism of an INI key, as `section/key` (any case).
    pub fn classify_ini_key(&self, game: &str, section_key: &str) -> Classified<'_> {
        self.classify(game, section_key, Field::IniKeys, false)
    }

    fn classify(&self, game: &str, text: &str, field: Field, not_game_data: bool) -> Classified<'_> {
        let lower = text.replace('\\', "/").to_lowercase();
        let path: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
        let mut best: Option<(&Mechanism, usize)> = None;
        for mechanism in self.mechanisms.iter().filter(|m| m.exists_for(game)) {
            let patterns = match field {
                Field::Paths => &mechanism.paths,
                Field::GameDir => &mechanism.game_dir_paths,
                Field::IniKeys => &mechanism.ini_keys,
            };
            for pattern in patterns.iter().filter(|p| p.matches(&path)) {
                if best.is_none_or(|(_, literal)| pattern.literal > literal) {
                    best = Some((mechanism, pattern.literal));
                }
            }
        }
        match best {
            Some((mechanism, _)) => Classified::Mechanism(mechanism),
            None if not_game_data && self.not_game_data.iter().any(|p| p.matches(&path)) => Classified::NotGameData,
            None => Classified::Unclassified,
        }
    }
}

/// The `section/key` names a game-format INI sets, lowercase, in file order; comments and blank lines skipped.
pub fn ini_keys(text: &str) -> Vec<String> {
    let mut section = String::new();
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = name.trim().to_lowercase();
        } else if let Some((key, _)) = line.split_once('=') {
            keys.push(format!("{section}/{}", key.trim().to_lowercase()));
        }
    }
    keys
}

#[cfg(test)]
#[path = "mod_catalog_tests.rs"]
mod tests;
