//! What translation produces and the controller plays (`docs/engine/music.md` §2), and the music
//! files a directive's paths resolve against.

use std::collections::BTreeSet;

/// The record or keyword a directive came from, for `music.status` (MUS-RPT-001).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Source {
    /// The EditorID, or a description where there is no record (an Oblivion category).
    pub(crate) label: String,
    pub(crate) form: Option<u32>,
    /// Which field named it (`cell XCMO`, `DOBJ Default Music`, ...).
    pub(crate) via: &'static str,
}

impl Source {
    pub(crate) fn record(label: impl Into<String>, form: u32, via: &'static str) -> Self {
        Self { label: label.into(), form: Some(form), via }
    }

    pub(crate) fn keyword(label: impl Into<String>, via: &'static str) -> Self {
        Self { label: label.into(), form: None, via }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.form {
            Some(form) => write!(f, "{} ({form:#010x}, {})", self.label, self.via),
            None => write!(f, "{} ({})", self.label, self.via),
        }
    }
}

/// One New Vegas Location-set track, played with the others (`docs/engine/music.md` §6).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stem {
    pub(crate) key: String,
    pub(crate) level: f32,
    pub(crate) night: bool,
    /// The share of the area's radius within which it plays (0..=1).
    pub(crate) reach: f32,
}

/// The area a set of stems belongs to: an audio marker's sphere, and its controller's day.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Area {
    /// Z-up world units, as authored.
    pub(crate) center: [f32; 3],
    pub(crate) radius: f32,
    pub(crate) day_start: u32,
    pub(crate) night_start: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    /// Archive-style keys (`music\explore\a.mp3`), played one after another.
    Playlist(Vec<String>),
    Stems { stems: Vec<Stem>, area: Area },
    Silent,
    Unresolved(String),
    Missing(String),
}

/// PRD §3.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Playing,
    Silent,
    Unresolved,
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Directive {
    pub(crate) source: Source,
    pub(crate) content: Content,
    pub(crate) looping: bool,
    /// Linear level, before the music volume.
    pub(crate) level: f32,
    pub(crate) fade_secs: f32,
    /// Script layer: paused while Battle plays (MUS-CMB-004).
    pub(crate) gives_way_to_battle: bool,
    /// Battle layer: how long it holds after combat ends (MUS-CMB-002).
    pub(crate) recovery_secs: Option<f32>,
    /// Battle layer: played once on entering and on leaving combat (MUS-CMB-003).
    pub(crate) intro: Option<String>,
    pub(crate) outro: Option<String>,
    /// What else `music.status` says about the source (New Vegas relation sets, Q3).
    pub(crate) notes: Vec<String>,
}

impl Directive {
    pub(crate) fn new(source: Source, content: Content) -> Self {
        Self {
            source,
            content,
            looping: false,
            level: 1.0,
            fade_secs: 0.0,
            gives_way_to_battle: false,
            recovery_secs: None,
            intro: None,
            outro: None,
            notes: Vec::new(),
        }
    }

    pub(crate) fn unresolved(source: Source, reason: impl Into<String>) -> Self {
        Self::new(source, Content::Unresolved(reason.into()))
    }

    pub(crate) fn outcome(&self) -> Outcome {
        match self.content {
            Content::Playlist(_) | Content::Stems { .. } => Outcome::Playing,
            Content::Silent => Outcome::Silent,
            Content::Unresolved(_) => Outcome::Unresolved,
            Content::Missing(_) => Outcome::Missing,
        }
    }

    /// Whether `other` asks for the same music: moving between two places that do must not
    /// restart the track (MUS-SEL-005).
    pub(crate) fn same_music(&self, other: &Self) -> bool {
        self.source.label == other.source.label && self.source.form == other.source.form && self.content == other.content
    }
}

/// The formats the streaming decoder plays (PRD §3.5 row 2).
const PLAYABLE: [&str; 4] = ["mp3", "wav", "ogg", "flac"];

fn playable(key: &str) -> bool {
    key.rsplit_once('.').is_some_and(|(_, ext)| PLAYABLE.iter().any(|p| ext.eq_ignore_ascii_case(p)))
}

/// Every music file the install has: the sound provider's `music\` keys, loose first, then the
/// archives (MUS-SRC-011, MUS-SRC-013).
#[derive(Debug, Default, Clone)]
pub(crate) struct MusicFiles {
    keys: BTreeSet<String>,
}

impl MusicFiles {
    pub(crate) fn new(keys: impl IntoIterator<Item = String>) -> Self {
        Self { keys: keys.into_iter().collect() }
    }

    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    /// A path relative to `Data\Music` as a directive's content: one file; a folder form's playable
    /// files directly in it (MUS-SRC-010); Silent for no path; Missing naming the path for an absent
    /// file or an empty folder (MUS-SRC-012); Unresolved for a file in a format the decoder lacks.
    pub(crate) fn resolve(&self, relative: &str) -> Content {
        let relative = relative.trim();
        if relative.is_empty() {
            return Content::Silent;
        }
        let normalized = relative.replace('/', "\\").to_ascii_lowercase();
        let normalized = normalized.trim_start_matches('\\');
        let shown = format!("Data\\Music\\{relative}");
        if normalized.ends_with('\\') {
            let prefix = format!("music\\{normalized}");
            let files: Vec<String> = self
                .keys
                .range(prefix.clone()..)
                .take_while(|key| key.starts_with(&prefix))
                .filter(|key| !key[prefix.len()..].contains('\\') && playable(key))
                .cloned()
                .collect();
            return if files.is_empty() { Content::Missing(shown) } else { Content::Playlist(files) };
        }
        let key = format!("music\\{normalized}");
        match (self.keys.contains(&key), playable(&key)) {
            (true, true) => Content::Playlist(vec![key]),
            (true, false) => Content::Unresolved(format!("{shown} is in a format the decoder does not play")),
            (false, _) => Content::Missing(shown),
        }
    }
}

/// A shuffled playlist order (Q1): every track plays once before any repeats, and a new round never
/// starts on the track the last one ended on (MUS-SEL-001).
#[derive(Debug, Clone, Default)]
pub(crate) struct Shuffle {
    /// The rest of this round, taken from the end.
    order: Vec<usize>,
    last: Option<usize>,
    rng: u64,
}

impl Shuffle {
    pub(crate) fn seeded(seed: u64) -> Self {
        Self { order: Vec::new(), last: None, rng: seed | 1 }
    }

    fn random(&mut self) -> u64 {
        // xorshift64*: enough for a playlist order.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// The next track of a playlist of `len`.
    pub(crate) fn next(&mut self, len: usize) -> usize {
        if len <= 1 {
            return 0;
        }
        if self.order.is_empty() || self.order.iter().any(|&i| i >= len) {
            self.order = (0..len).collect();
            for i in (1..len).rev() {
                let j = (self.random() % (i as u64 + 1)) as usize;
                self.order.swap(i, j);
            }
            if self.order.last() == self.last.as_ref() {
                self.order.swap(0, len - 1);
            }
        }
        let next = self.order.pop().expect("refilled above");
        self.last = Some(next);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> MusicFiles {
        MusicFiles::new(
            ["music\\explore\\a.mp3", "music\\explore\\b.mp3", "music\\explore\\sub\\c.mp3", "music\\explore\\notes.txt", "music\\special\\death.mp3", "music\\dungeon\\x.xma"]
                .map(String::from),
        )
    }

    #[test]
    fn a_folder_form_expands_to_the_playable_files_directly_in_it() {
        assert_eq!(
            files().resolve("Explore\\"),
            Content::Playlist(vec!["music\\explore\\a.mp3".into(), "music\\explore\\b.mp3".into()])
        );
        assert_eq!(files().resolve("special/death.mp3"), Content::Playlist(vec!["music\\special\\death.mp3".into()]));
    }

    #[test]
    fn absent_paths_are_missing_by_name_and_no_path_is_silent() {
        assert_eq!(files().resolve("public\\"), Content::Missing("Data\\Music\\public\\".into()));
        assert_eq!(files().resolve("special\\success.mp3"), Content::Missing("Data\\Music\\special\\success.mp3".into()));
        assert_eq!(files().resolve(""), Content::Silent);
        assert!(matches!(files().resolve("dungeon\\x.xma"), Content::Unresolved(_)));
        assert_eq!(files().resolve("dungeon\\"), Content::Missing("Data\\Music\\dungeon\\".into()), "no playable file in it");
    }

    #[test]
    fn a_shuffle_plays_every_track_before_repeating_and_never_restarts_on_the_last() {
        let mut shuffle = Shuffle::seeded(7);
        let mut previous = None;
        for _ in 0..50 {
            let mut round: Vec<usize> = (0..5).map(|_| shuffle.next(5)).collect();
            assert_ne!(Some(round[0]), previous, "a round never starts on the track the last one ended on");
            previous = round.last().copied();
            round.sort_unstable();
            assert_eq!(round, [0, 1, 2, 3, 4]);
        }
        assert_eq!(Shuffle::seeded(1).next(1), 0, "a one-track playlist is that track");
    }
}
