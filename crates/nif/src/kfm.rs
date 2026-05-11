//! KFM (KeyFrame Metadata) file parser — animation state machines.
//!
//! A KFM file defines the relationship between a mesh's KF animation
//! clips: which sequences exist, what their IDs are, which files they
//! come from, how they transition between each other, and how they
//! combine via synchronization groups. Gamebryo's animation tools use
//! this as the top-level catalog that `NiKFMTool` loads at runtime.
//!
//! # Scope
//!
//! This parser targets the **binary** KFM format, versions 1.2.0.0 through
//! 2.2.0.0, which covers the Gamebryo 2.3 era — Oblivion, Fallout 3,
//! Fallout New Vegas. ASCII KFM support is deliberately omitted because
//! Gamebryo's reference implementation (`NiKFMTool::ReadAscii`) rejects
//! it with `KFM_ERR_FILE_FORMAT` — no shipped game asset uses text KFM.
//!
//! The wire format was reverse-engineered from the authoritative
//! `NiKFMTool::ReadBinary` implementation in the Gamebryo 2.3 source,
//! with every version gate (`uiVersion < GetVersion(X, Y, Z, W)`)
//! preserved. See issue #79.
//!
//! # What this does NOT do
//!
//! - Does not load the referenced .nif / .kf files. Consumers resolve
//!   those via the existing `parse_nif` + `import_kf` paths using the
//!   filenames returned here.
//! - Does not run the state machine. Runtime transition evaluation is
//!   the animation system's job — this is a pure catalog / metadata
//!   layer, and the existing `byroredux_core::animation::AnimationStack`
//!   already provides programmatic blend control.
//! - Does not parse the obsolete "old version" (< 1.2.0.0) format.
//!
//! # Example
//!
//! ```ignore
//! let bytes = std::fs::read("character.kfm")?;
//! let kfm = byroredux_nif::kfm::parse_kfm(&bytes)?;
//! println!("Model: {}", kfm.model_path);
//! for seq in &kfm.sequences {
//!     println!("  [{}] {} → {}", seq.sequence_id, seq.name, seq.filename);
//! }
//! ```

use std::io::{self, Read};

/// Parsed KFM file — the full animation-catalog contents.
#[derive(Debug, Clone, Default)]
pub struct KfmFile {
    /// Version triplet decoded from the header line (e.g. `(2, 2, 0, 0)`).
    pub version: (u8, u8, u8, u8),
    /// Little-endian flag read from the 1-byte endianness marker
    /// present since KFM v1.2.6.0. `true` for little-endian files,
    /// which is the universal case on Bethesda-shipped assets.
    pub little_endian: bool,
    /// Relative path to the model NIF this KFM is scoped to.
    pub model_path: String,
    /// Name of the root scene-graph node that should receive the
    /// animations (empty when the KFM addresses the whole model).
    pub model_root: String,
    /// Default transition applied between sync-compatible sequences.
    pub default_sync_transition: KfmTransitionDefaults,
    /// Default transition applied between sync-incompatible sequences.
    pub default_nonsync_transition: KfmTransitionDefaults,
    /// Catalog of sequences. Ordered by file position, not by
    /// `sequence_id`.
    pub sequences: Vec<KfmSequence>,
    /// Sequence groups used for synchronized multi-clip playback
    /// (e.g. upper-body + lower-body sync).
    pub sequence_groups: Vec<KfmSequenceGroup>,
}

/// Defaults for one of the two transition categories (sync / nonsync).
#[derive(Debug, Clone, Copy, Default)]
pub struct KfmTransitionDefaults {
    pub transition_type: KfmTransitionType,
    pub duration: f32,
}

/// Transition blend mode. Values match the `NiKFMTool::TransitionType`
/// enum in the Gamebryo 2.3 source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KfmTransitionType {
    #[default]
    Blend,
    Morph,
    Crossfade,
    Chain,
    DefaultSync,
    DefaultNonSync,
    DefaultInvalid,
    /// Unknown enum value — the parser carries it through unchanged so
    /// pathological files don't crash the importer.
    Unknown(i32),
}

impl KfmTransitionType {
    fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Blend,
            1 => Self::Morph,
            2 => Self::Crossfade,
            3 => Self::Chain,
            4 => Self::DefaultSync,
            5 => Self::DefaultNonSync,
            6 => Self::DefaultInvalid,
            other => Self::Unknown(other),
        }
    }
}

/// A single named animation sequence in the catalog.
#[derive(Debug, Clone)]
pub struct KfmSequence {
    /// Stable integer identifier used by transitions and sequence groups.
    pub sequence_id: u32,
    /// Path to the .kf file that holds this sequence's keyframe data.
    pub filename: String,
    /// Legacy animation index from KFM < 1.2.5.0. Still present on the
    /// wire in newer versions for compatibility but usually `-1`.
    pub anim_index: i32,
    /// Transitions from this sequence keyed by destination sequence ID.
    pub transitions: Vec<KfmTransition>,
}

/// A transition from one sequence to another.
#[derive(Debug, Clone)]
pub struct KfmTransition {
    pub dest_sequence_id: u32,
    pub transition_type: KfmTransitionType,
    /// Blend duration in seconds. Zero when `transition_type` is
    /// `DefaultSync` / `DefaultNonSync` — those inherit from the
    /// top-level default blocks.
    pub duration: f32,
    /// Pairs of (start text-key, target text-key) used for morph /
    /// crossfade alignment. Empty for most transitions.
    pub blend_pairs: Vec<KfmBlendPair>,
    /// Chain of sequences to play in succession to reach the
    /// destination. Used for complex multi-step transitions.
    pub chain: Vec<KfmChainEntry>,
}

/// A single (start, target) text-key pairing within a transition.
#[derive(Debug, Clone)]
pub struct KfmBlendPair {
    pub start_key: String,
    pub target_key: String,
}

/// One chained-transition step: a sequence to play for a given duration.
#[derive(Debug, Clone, Copy)]
pub struct KfmChainEntry {
    pub sequence_id: u32,
    pub duration: f32,
}

/// A synchronization group binding multiple sequences to share a
/// common playback clock (upper-body + lower-body sync, for example).
#[derive(Debug, Clone)]
pub struct KfmSequenceGroup {
    pub group_id: u32,
    pub name: String,
    pub members: Vec<KfmSequenceGroupMember>,
}

/// One sequence's participation in a sync group.
#[derive(Debug, Clone, Copy)]
pub struct KfmSequenceGroupMember {
    pub sequence_id: u32,
    pub priority: i32,
    pub weight: f32,
    pub ease_in_time: f32,
    pub ease_out_time: f32,
    /// ID of another member to synchronize phase with.
    /// `SYNC_SEQUENCE_ID_NONE` (= `u32::MAX`) means no sync.
    pub synchronize_sequence_id: u32,
}

/// Sentinel value for "no sync partner" — matches the
/// `NiKFMTool::SYNC_SEQUENCE_ID_NONE` constant.
pub const SYNC_SEQUENCE_ID_NONE: u32 = u32::MAX;

/// Lookup-augmented wrapper around a parsed [`KfmFile`] — the bridge
/// between a per-actor `.kfm` (`character.kfm`, `creature.kfm`,
/// `talkingactivator.kfm`, …) and the gameplay code that requests
/// sequences by name (`"1hmidle"`, `"runforward"`, …).
///
/// The parser ([`KfmFile`]) leaves the catalog as raw vectors ordered
/// by file position; resolving a sequence by editor name therefore
/// requires an O(n) linear scan plus a per-call filename-stem extraction
/// every time a gameplay event ticks. [`KfmCatalog`] precomputes that
/// index once at load and keeps the underlying [`KfmFile`] available
/// for direct access to transitions / sync groups / defaults.
///
/// The "name" used by [`find_by_name`](Self::find_by_name) is the
/// stem of `KfmSequence.filename` lowercased:
///
/// ```text
///   "characters\\_male\\idleanims\\1hmidle.kf"  →  "1hmidle"
///   "1stperson\\1hmidle.KF"                     →  "1hmidle"
/// ```
///
/// Matches both forward- and back-slash path separators (KFM files
/// historically ship with Windows-style `\` on Bethesda content but
/// editor tools occasionally normalise to `/`). Case-insensitive
/// because Bethesda gameplay scripts request sequences in
/// inconsistent casing.
///
/// Per [#532](https://github.com/matiaszanolli/ByroRedux/issues/532)
/// (FNV-ANIM-4) this closes the import-side half of the wiring gap;
/// the actor-controller half (`AnimationController` population +
/// `.kf` clip loading) is M30 milestone work that builds on this
/// catalog without touching the parser.
#[derive(Debug, Clone)]
pub struct KfmCatalog {
    /// The underlying parsed KFM document. Held verbatim so callers
    /// retain access to transitions, sequence groups, default
    /// transition durations, and the model path / root node.
    pub file: KfmFile,
    /// Lowercased filename-stem → index into [`file.sequences`].
    /// Built once at construction; an O(1) hash lookup per gameplay
    /// event vs. the parser's O(n) linear scan.
    name_to_index: std::collections::HashMap<String, usize>,
    /// `sequence_id` → index into [`file.sequences`]. KFM stores
    /// sequences in file-position order, NOT id order, so an
    /// `AnimationController::add_sequence` walk that visits the
    /// transition table by `dest_sequence_id` would otherwise need
    /// its own linear scan per transition.
    id_to_index: std::collections::HashMap<u32, usize>,
}

impl KfmCatalog {
    /// Build the catalog from a parsed [`KfmFile`]. Both indexes are
    /// populated eagerly so subsequent lookups are O(1).
    ///
    /// Duplicate `sequence_id` values (pathological KFM authoring; the
    /// reference NiKFMTool rejects them) keep the FIRST occurrence's
    /// index — the runtime then surfaces the duplicate as silently
    /// ignored rather than panicking, matching the parser's
    /// graceful-degradation contract. Duplicate filename stems are
    /// handled the same way.
    pub fn new(file: KfmFile) -> Self {
        let mut name_to_index = std::collections::HashMap::with_capacity(file.sequences.len());
        let mut id_to_index = std::collections::HashMap::with_capacity(file.sequences.len());
        for (idx, seq) in file.sequences.iter().enumerate() {
            id_to_index.entry(seq.sequence_id).or_insert(idx);
            let stem = filename_stem_lower(&seq.filename);
            if !stem.is_empty() {
                name_to_index.entry(stem).or_insert(idx);
            }
        }
        Self {
            file,
            name_to_index,
            id_to_index,
        }
    }

    /// Parse + index in one call. Equivalent to
    /// `KfmCatalog::new(parse_kfm(bytes)?)` but reads more naturally
    /// at the BSA-extract → catalog call site.
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        Ok(Self::new(parse_kfm(bytes)?))
    }

    /// Look up a sequence by its editor-name (the stem of
    /// `KfmSequence.filename`, case-insensitive). See the type-level
    /// doc for the stem-extraction rules.
    ///
    /// Returns `None` when the name doesn't match any sequence. The
    /// resolved reference borrows from [`file.sequences`] so callers
    /// reading `filename` / `transitions` / `anim_index` don't pay a
    /// clone.
    pub fn find_by_name(&self, name: &str) -> Option<&KfmSequence> {
        let key = name.trim().to_ascii_lowercase();
        let &idx = self.name_to_index.get(&key)?;
        self.file.sequences.get(idx)
    }

    /// Look up a sequence by its stable `sequence_id`. Mirror of
    /// [`find_by_name`](Self::find_by_name) used by transition-table
    /// resolution where the destination is keyed by id (per
    /// `KfmTransition::dest_sequence_id`).
    pub fn find_by_id(&self, sequence_id: u32) -> Option<&KfmSequence> {
        let &idx = self.id_to_index.get(&sequence_id)?;
        self.file.sequences.get(idx)
    }

    /// Iterator over `(name_stem, &KfmSequence)` pairs in
    /// sequence-id-sorted order. The M30 actor-controller bring-up
    /// uses this to walk the catalog once and populate
    /// `AnimationController` + a per-actor name→clip_handle map
    /// without paying the linear sequence-by-name scan per insert.
    pub fn iter_named(&self) -> impl Iterator<Item = (&str, &KfmSequence)> {
        // Pre-sorted by stable sequence_id so consumers get deterministic
        // iteration regardless of file authoring order.
        let mut ordered: Vec<(&str, usize)> = self
            .name_to_index
            .iter()
            .map(|(k, &v)| (k.as_str(), v))
            .collect();
        ordered.sort_by_key(|(_, idx)| self.file.sequences[*idx].sequence_id);
        ordered
            .into_iter()
            .map(move |(name, idx)| (name, &self.file.sequences[idx]))
    }
}

/// Strip the parent path and extension off a `KfmSequence.filename`
/// and lowercase the remainder. Handles both `\` and `/` separators;
/// returns an empty string when the input is empty or all-separator.
fn filename_stem_lower(path: &str) -> String {
    // Split on the LAST `\` or `/` — whichever comes later.
    let last_sep = path.rfind(|c| c == '\\' || c == '/');
    let leaf = match last_sep {
        Some(i) => &path[i + 1..],
        None => path,
    };
    // Strip the LAST `.` and everything after it (the extension), if any.
    let stem = match leaf.rfind('.') {
        Some(i) => &leaf[..i],
        None => leaf,
    };
    stem.to_ascii_lowercase()
}

/// Minimum KFM version we parse (`NiKFMTool::LoadFile` rejects older
/// files as "old version ASCII" which this parser deliberately skips).
const MIN_VERSION: (u8, u8, u8, u8) = (1, 2, 0, 0);

/// Maximum KFM version the Gamebryo 2.3 source ships with. We accept
/// anything up to this inclusive; newer files (if they ever exist)
/// will be rejected early so the parser doesn't silently mis-read.
const MAX_VERSION: (u8, u8, u8, u8) = (2, 2, 0, 0);

fn pack_version(v: (u8, u8, u8, u8)) -> u32 {
    ((v.0 as u32) << 24) | ((v.1 as u32) << 16) | ((v.2 as u32) << 8) | (v.3 as u32)
}

/// Parse a binary KFM file from a byte slice. See the module docs for
/// scope + the Gamebryo 2.3 source reference. Returns `Err` for ASCII
/// files, old pre-1.2.0.0 files, and unknown header lines.
pub fn parse_kfm(bytes: &[u8]) -> io::Result<KfmFile> {
    // ── Header line ────────────────────────────────────────────────
    // Format: ";Gamebryo KFM File Version X.Y.Z.Wb\n" — trailing 'b'
    // means binary, 'a' means ASCII. Line length is bounded by 255
    // characters in the reference implementation.
    const HEADER_PREFIX: &[u8] = b";Gamebryo KFM File Version ";
    if !bytes.starts_with(HEADER_PREFIX) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a KFM file (missing ';Gamebryo KFM File Version ' header)",
        ));
    }

    // Find the newline terminating the header line.
    let nl_pos = bytes
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "KFM header has no newline"))?;
    let header_line = &bytes[HEADER_PREFIX.len()..nl_pos];
    // Strip trailing \r if CRLF (rare but legal).
    let header_line = if header_line.last() == Some(&b'\r') {
        &header_line[..header_line.len() - 1]
    } else {
        header_line
    };
    if header_line.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "KFM header has no version string",
        ));
    }

    // Parse the version — last character is the format marker ('a' or 'b').
    let format_marker = header_line[header_line.len() - 1];
    let binary = match format_marker {
        b'b' => true,
        b'a' => false,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "KFM header ends with unexpected format marker 0x{:02x}",
                    format_marker
                ),
            ))
        }
    };
    if !binary {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ASCII KFM files are not supported (Gamebryo 2.3 reference \
             also rejects them with KFM_ERR_FILE_FORMAT)",
        ));
    }
    let version_str = std::str::from_utf8(&header_line[..header_line.len() - 1])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "KFM version not valid UTF-8"))?;
    let version = parse_version_triplet(version_str)?;

    if pack_version(version) < pack_version(MIN_VERSION) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "KFM version {:?} is older than the minimum supported ({:?}) — \
                 pre-1.2.0.0 uses a different wire layout that the Gamebryo 2.3 \
                 reference routes through ReadOldVersionAscii",
                version, MIN_VERSION
            ),
        ));
    }
    if pack_version(version) > pack_version(MAX_VERSION) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "KFM version {:?} is newer than the maximum supported ({:?})",
                version, MAX_VERSION
            ),
        ));
    }

    let mut r = KfmReader::new(&bytes[nl_pos + 1..]);

    // ── Endianness byte (since 1.2.6.0) ────────────────────────────
    // Version 1.2.6.0 is > our MAX_VERSION (2.2.0.0 ships at 1.2.6
    // boundary? — actually 1.2.6.0 < 2.2.0.0), so this path is live
    // for every file at or above 1.2.6.0. Pre-1.2.6.0 assumes
    // little-endian on disk.
    let little_endian = if pack_version(version) >= pack_version((1, 2, 6, 0)) {
        r.read_bool()?
    } else {
        true
    };
    if !little_endian {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "big-endian KFM files are not supported",
        ));
    }

    // ── Legacy default paths (< 1.2.3.0) ───────────────────────────
    if pack_version(version) < pack_version((1, 2, 3, 0)) {
        let has_default_paths = r.read_bool()?;
        if has_default_paths {
            // Skip the default NIF + default KF path strings — the
            // reference implementation uses them to rewrite relative
            // paths but we report paths as-is.
            let _default_nif = r.read_cstring()?;
            let _default_kf = r.read_cstring()?;
        }
    }

    // ── Model path (CString) ───────────────────────────────────────
    let model_path = r.read_cstring()?;

    // ── Model root ─────────────────────────────────────────────────
    // CString pre-2.1.0.0, FixedString from 2.1.0.0 onward. Both share
    // the same wire layout (`i32 length` + bytes) in the reference so
    // we can read either with the same helper.
    let model_root = r.read_cstring()?;

    // ── Default transition settings (since 1.2.2.0) ────────────────
    let (default_sync_transition, default_nonsync_transition) =
        if pack_version(version) >= pack_version((1, 2, 2, 0)) {
            let sync_type = KfmTransitionType::from_i32(r.read_i32_le()?);
            let nonsync_type = KfmTransitionType::from_i32(r.read_i32_le()?);
            let sync_duration = r.read_f32_le()?;
            let nonsync_duration = r.read_f32_le()?;
            (
                KfmTransitionDefaults {
                    transition_type: sync_type,
                    duration: sync_duration,
                },
                KfmTransitionDefaults {
                    transition_type: nonsync_type,
                    duration: nonsync_duration,
                },
            )
        } else {
            (
                KfmTransitionDefaults::default(),
                KfmTransitionDefaults::default(),
            )
        };

    // ── Sequences ──────────────────────────────────────────────────
    let num_sequences = r.read_u32_le()?;
    let mut sequences = r.allocate_vec::<KfmSequence>(num_sequences)?;
    for _ in 0..num_sequences {
        let sequence_id = r.read_u32_le()?;

        // Legacy name field (< 1.2.5.0): consume and discard.
        if pack_version(version) < pack_version((1, 2, 5, 0)) {
            let _legacy_name = r.read_cstring()?;
        }

        let filename = r.read_cstring()?;
        let anim_index = r.read_i32_le()?;

        // Transitions.
        let num_transitions = r.read_u32_le()?;
        let mut transitions = r.allocate_vec::<KfmTransition>(num_transitions)?;
        for _ in 0..num_transitions {
            let dest_sequence_id = r.read_u32_le()?;
            let transition_type = KfmTransitionType::from_i32(r.read_i32_le()?);

            // DEFAULT_SYNC / DEFAULT_NONSYNC transitions reference the
            // top-level default block and carry no per-transition data.
            if matches!(
                transition_type,
                KfmTransitionType::DefaultSync | KfmTransitionType::DefaultNonSync
            ) {
                transitions.push(KfmTransition {
                    dest_sequence_id,
                    transition_type,
                    duration: 0.0,
                    blend_pairs: Vec::new(),
                    chain: Vec::new(),
                });
                continue;
            }

            let duration = r.read_f32_le()?;

            // Blend pairs.
            let num_blend_pairs = r.read_u32_le()?;
            let mut blend_pairs = r.allocate_vec::<KfmBlendPair>(num_blend_pairs)?;
            for _ in 0..num_blend_pairs {
                let start_key = r.read_cstring()?;
                let target_key = r.read_cstring()?;
                blend_pairs.push(KfmBlendPair {
                    start_key,
                    target_key,
                });
            }

            // Chain info. Pre-1.2.4.0 files stored the source sequence
            // as the first entry — the reference code skips it to keep
            // the semantics consistent with newer files.
            let mut num_chain = r.read_u32_le()?;
            if pack_version(version) < pack_version((1, 2, 4, 0)) && num_chain > 0 {
                let _legacy_src_seq = r.read_u32_le()?;
                let _legacy_src_dur = r.read_f32_le()?;
                num_chain -= 1;
            }
            let mut chain = r.allocate_vec::<KfmChainEntry>(num_chain)?;
            for _ in 0..num_chain {
                let seq = r.read_u32_le()?;
                let dur = r.read_f32_le()?;
                chain.push(KfmChainEntry {
                    sequence_id: seq,
                    duration: dur,
                });
            }

            transitions.push(KfmTransition {
                dest_sequence_id,
                transition_type,
                duration,
                blend_pairs,
                chain,
            });
        }

        sequences.push(KfmSequence {
            sequence_id,
            filename,
            anim_index,
            transitions,
        });
    }

    // ── Sequence groups ────────────────────────────────────────────
    let num_groups = r.read_u32_le()?;
    let mut sequence_groups = r.allocate_vec::<KfmSequenceGroup>(num_groups)?;
    for _ in 0..num_groups {
        let group_id = r.read_u32_le()?;
        let name = r.read_cstring()?;
        let num_members = r.read_u32_le()?;
        let mut members = r.allocate_vec::<KfmSequenceGroupMember>(num_members)?;
        for _ in 0..num_members {
            let sequence_id = r.read_u32_le()?;
            let priority = r.read_i32_le()?;
            let weight = r.read_f32_le()?;
            let ease_in_time = r.read_f32_le()?;
            let ease_out_time = r.read_f32_le()?;
            let synchronize_sequence_id = if pack_version(version) >= pack_version((1, 2, 1, 0)) {
                r.read_u32_le()?
            } else {
                SYNC_SEQUENCE_ID_NONE
            };
            members.push(KfmSequenceGroupMember {
                sequence_id,
                priority,
                weight,
                ease_in_time,
                ease_out_time,
                synchronize_sequence_id,
            });
        }
        sequence_groups.push(KfmSequenceGroup {
            group_id,
            name,
            members,
        });
    }

    Ok(KfmFile {
        version,
        little_endian,
        model_path,
        model_root,
        default_sync_transition,
        default_nonsync_transition,
        sequences,
        sequence_groups,
    })
}

/// Parse a `"X.Y.Z.W"` version triplet into the `(u8, u8, u8, u8)` tuple.
fn parse_version_triplet(s: &str) -> io::Result<(u8, u8, u8, u8)> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("KFM version `{s}` does not have 4 dot-separated fields"),
        ));
    }
    let mut out = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p.parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("KFM version field `{p}` is not a valid u8"),
            )
        })?;
    }
    Ok((out[0], out[1], out[2], out[3]))
}

/// Minimal little-endian binary reader used by the KFM parser. We
/// deliberately don't share `NifStream` here because KFM has no
/// version-gated primitive size changes and needs no NIF header.
struct KfmReader<'a> {
    cursor: io::Cursor<&'a [u8]>,
}

impl<'a> KfmReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: io::Cursor::new(bytes),
        }
    }

    fn read_u32_le(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_i32_le(&mut self) -> io::Result<i32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }

    fn read_f32_le(&mut self) -> io::Result<f32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }

    fn read_bool(&mut self) -> io::Result<bool> {
        let mut buf = [0u8; 1];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf[0] != 0)
    }

    /// Bound a file-driven `Vec::with_capacity(count)` against the
    /// remaining bytes in the KFM blob. KFM headers carry several
    /// `u32` counts (sequences, transitions, blend pairs, chain
    /// entries, sequence groups) — without this guard a corrupt count
    /// would trip a multi-GB allocation. The bound is on-disk bytes
    /// (each element is at least one byte on disk), not
    /// `size_of::<T>()` — see the equivalent rationale on
    /// [`NifStream::allocate_vec`]. See #388.
    fn allocate_vec<T>(&self, count: u32) -> io::Result<Vec<T>> {
        let pos = self.cursor.position() as usize;
        let remaining = self.cursor.get_ref().len().saturating_sub(pos);
        if (count as usize) > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "KFM claims {count} entries but only {remaining} bytes remain at position {pos}"
                ),
            ));
        }
        Ok(Vec::with_capacity(count as usize))
    }

    /// Read an `i32` length prefix followed by that many bytes as a
    /// UTF-8 string. Matches `NiKFMTool::LoadCString` /
    /// `LoadFixedString` / `LoadCStringAsFixedString`, which all use
    /// the same on-disk layout despite different in-memory types.
    /// A length of `0` returns an empty string; a negative length is
    /// treated as a parse error.
    fn read_cstring(&mut self) -> io::Result<String> {
        let len = self.read_i32_le()?;
        if len == 0 {
            return Ok(String::new());
        }
        if len < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("KFM string has negative length {len}"),
            ));
        }
        let mut buf = vec![0u8; len as usize];
        self.cursor.read_exact(&mut buf)?;
        // Strip any trailing null byte — the reference serializer does
        // not write one, but some tools do.
        if buf.last() == Some(&0) {
            buf.pop();
        }
        String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but complete binary KFM v2.2.0.0 blob: one
    /// sequence with one chained transition to a second (empty)
    /// sequence, plus one sequence group linking both.
    fn sample_kfm_2_2_0_0() -> Vec<u8> {
        let mut d = Vec::new();
        // Header line
        d.extend_from_slice(b";Gamebryo KFM File Version 2.2.0.0b\n");
        // Endianness: little-endian
        d.push(0x01);
        // Model path
        write_cstring(&mut d, "meshes\\character.nif");
        // Model root (fixed string — same layout at 2.2.0.0)
        write_cstring(&mut d, "Bip01");
        // Default sync transition: TYPE_BLEND (0), duration 0.25
        d.extend_from_slice(&0i32.to_le_bytes());
        // Default nonsync transition: TYPE_CROSSFADE (2), duration 0.5
        d.extend_from_slice(&2i32.to_le_bytes());
        d.extend_from_slice(&0.25_f32.to_le_bytes());
        d.extend_from_slice(&0.5_f32.to_le_bytes());
        // num_sequences = 2
        d.extend_from_slice(&2u32.to_le_bytes());

        // Sequence 0: "Idle" — 1 transition to seq 1, TYPE_BLEND, 0.2s,
        //   0 blend pairs, 2 chain entries.
        d.extend_from_slice(&0u32.to_le_bytes()); // sequence_id
        write_cstring(&mut d, "idle.kf");
        d.extend_from_slice(&(-1i32).to_le_bytes()); // anim_index
        d.extend_from_slice(&1u32.to_le_bytes()); // num_transitions
        d.extend_from_slice(&1u32.to_le_bytes()); // dest_id
        d.extend_from_slice(&0i32.to_le_bytes()); // type = BLEND
        d.extend_from_slice(&0.2_f32.to_le_bytes()); // duration
        d.extend_from_slice(&0u32.to_le_bytes()); // num_blend_pairs
        d.extend_from_slice(&2u32.to_le_bytes()); // num_chain
        d.extend_from_slice(&42u32.to_le_bytes());
        d.extend_from_slice(&0.1_f32.to_le_bytes());
        d.extend_from_slice(&7u32.to_le_bytes());
        d.extend_from_slice(&0.4_f32.to_le_bytes());

        // Sequence 1: "Walk" — 0 transitions.
        d.extend_from_slice(&1u32.to_le_bytes()); // sequence_id
        write_cstring(&mut d, "walk.kf");
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // num_transitions

        // num_groups = 1
        d.extend_from_slice(&1u32.to_le_bytes());
        // Group: id 0, name "Locomotion", 2 members (both seqs).
        d.extend_from_slice(&0u32.to_le_bytes());
        write_cstring(&mut d, "Locomotion");
        d.extend_from_slice(&2u32.to_le_bytes()); // num_members
                                                  // Member 0: seq 0, priority 10, weight 1.0, ease 0.1/0.2, no sync.
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&10i32.to_le_bytes());
        d.extend_from_slice(&1.0_f32.to_le_bytes());
        d.extend_from_slice(&0.1_f32.to_le_bytes());
        d.extend_from_slice(&0.2_f32.to_le_bytes());
        d.extend_from_slice(&u32::MAX.to_le_bytes()); // SYNC_SEQUENCE_ID_NONE
                                                      // Member 1: seq 1, priority 20, weight 0.5, ease 0.3/0.4, sync to 0.
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&20i32.to_le_bytes());
        d.extend_from_slice(&0.5_f32.to_le_bytes());
        d.extend_from_slice(&0.3_f32.to_le_bytes());
        d.extend_from_slice(&0.4_f32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());

        d
    }

    fn write_cstring(buf: &mut Vec<u8>, s: &str) {
        buf.extend_from_slice(&(s.len() as i32).to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
    }

    #[test]
    fn parses_sample_kfm_2_2_0_0() {
        let bytes = sample_kfm_2_2_0_0();
        let kfm = parse_kfm(&bytes).expect("should parse synthetic KFM v2.2.0.0");

        assert_eq!(kfm.version, (2, 2, 0, 0));
        assert!(kfm.little_endian);
        assert_eq!(kfm.model_path, "meshes\\character.nif");
        assert_eq!(kfm.model_root, "Bip01");

        assert_eq!(
            kfm.default_sync_transition.transition_type,
            KfmTransitionType::Blend
        );
        assert!((kfm.default_sync_transition.duration - 0.25).abs() < 1e-5);
        assert_eq!(
            kfm.default_nonsync_transition.transition_type,
            KfmTransitionType::Crossfade
        );
        assert!((kfm.default_nonsync_transition.duration - 0.5).abs() < 1e-5);

        assert_eq!(kfm.sequences.len(), 2);
        let idle = &kfm.sequences[0];
        assert_eq!(idle.sequence_id, 0);
        assert_eq!(idle.filename, "idle.kf");
        assert_eq!(idle.anim_index, -1);
        assert_eq!(idle.transitions.len(), 1);

        let t = &idle.transitions[0];
        assert_eq!(t.dest_sequence_id, 1);
        assert_eq!(t.transition_type, KfmTransitionType::Blend);
        assert!((t.duration - 0.2).abs() < 1e-5);
        assert!(t.blend_pairs.is_empty());
        assert_eq!(t.chain.len(), 2);
        assert_eq!(t.chain[0].sequence_id, 42);
        assert!((t.chain[0].duration - 0.1).abs() < 1e-5);
        assert_eq!(t.chain[1].sequence_id, 7);
        assert!((t.chain[1].duration - 0.4).abs() < 1e-5);

        let walk = &kfm.sequences[1];
        assert_eq!(walk.sequence_id, 1);
        assert_eq!(walk.filename, "walk.kf");
        assert!(walk.transitions.is_empty());

        assert_eq!(kfm.sequence_groups.len(), 1);
        let group = &kfm.sequence_groups[0];
        assert_eq!(group.group_id, 0);
        assert_eq!(group.name, "Locomotion");
        assert_eq!(group.members.len(), 2);
        assert_eq!(group.members[0].sequence_id, 0);
        assert_eq!(group.members[0].priority, 10);
        assert_eq!(
            group.members[0].synchronize_sequence_id,
            SYNC_SEQUENCE_ID_NONE
        );
        assert_eq!(group.members[1].sequence_id, 1);
        assert_eq!(group.members[1].synchronize_sequence_id, 0);
    }

    #[test]
    fn rejects_ascii_kfm() {
        let bytes = b";Gamebryo KFM File Version 2.2.0.0a\n";
        let err = parse_kfm(bytes).expect_err("ASCII KFM must be rejected");
        assert!(err.to_string().contains("ASCII KFM"));
    }

    #[test]
    fn rejects_unknown_header() {
        let bytes = b"not a KFM file at all\n";
        let err = parse_kfm(bytes).expect_err("garbage header must be rejected");
        assert!(err.to_string().contains("not a KFM file"));
    }

    #[test]
    fn rejects_missing_newline() {
        let bytes = b";Gamebryo KFM File Version 2.2.0.0b";
        let err = parse_kfm(bytes).expect_err("missing newline must be rejected");
        assert!(err.to_string().contains("no newline"));
    }

    #[test]
    fn rejects_version_above_max() {
        let bytes = b";Gamebryo KFM File Version 99.0.0.0b\n";
        let err = parse_kfm(bytes).expect_err("future version must be rejected");
        assert!(err.to_string().contains("newer than the maximum"));
    }

    #[test]
    fn rejects_version_below_min() {
        let bytes = b";Gamebryo KFM File Version 1.1.0.0b\n";
        let err = parse_kfm(bytes).expect_err("pre-1.2.0.0 must be rejected");
        assert!(err.to_string().contains("older than the minimum"));
    }

    #[test]
    fn rejects_bad_version_triplet() {
        let bytes = b";Gamebryo KFM File Version 2.2.0b\n";
        let err = parse_kfm(bytes).expect_err("3-field version must be rejected");
        assert!(err
            .to_string()
            .contains("does not have 4 dot-separated fields"));
    }

    #[test]
    fn rejects_big_endian_flag() {
        let mut bytes = b";Gamebryo KFM File Version 2.2.0.0b\n".to_vec();
        bytes.push(0x00); // little_endian = false
        let err = parse_kfm(&bytes).expect_err("big-endian KFM must be rejected");
        assert!(err.to_string().contains("big-endian"));
    }

    #[test]
    fn pack_version_ordering() {
        // Sanity: version compares work as the parser expects.
        assert!(pack_version((1, 2, 0, 0)) < pack_version((1, 2, 6, 0)));
        assert!(pack_version((1, 2, 6, 0)) < pack_version((2, 2, 0, 0)));
        assert!(pack_version((2, 2, 0, 0)) < pack_version((99, 0, 0, 0)));
    }

    #[test]
    fn default_sync_transition_has_no_per_transition_data() {
        // Build a sequence with a single `DefaultSync` transition and
        // verify the parser short-circuits the duration / blend pair /
        // chain reads.
        let mut d = Vec::new();
        d.extend_from_slice(b";Gamebryo KFM File Version 2.2.0.0b\n");
        d.push(0x01); // little-endian
        write_cstring(&mut d, "x.nif"); // model_path
        write_cstring(&mut d, ""); // model_root
                                   // Default transitions
        d.extend_from_slice(&0i32.to_le_bytes());
        d.extend_from_slice(&0i32.to_le_bytes());
        d.extend_from_slice(&0.0_f32.to_le_bytes());
        d.extend_from_slice(&0.0_f32.to_le_bytes());
        // 1 sequence
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // sequence_id
        write_cstring(&mut d, "idle.kf");
        d.extend_from_slice(&(-1i32).to_le_bytes()); // anim_index
        d.extend_from_slice(&1u32.to_le_bytes()); // num_transitions
        d.extend_from_slice(&1u32.to_le_bytes()); // dest_id
        d.extend_from_slice(&4i32.to_le_bytes()); // type = DEFAULT_SYNC
                                                  // No further bytes for this transition.
                                                  // 0 sequence groups
        d.extend_from_slice(&0u32.to_le_bytes());

        let kfm = parse_kfm(&d).expect("should parse DEFAULT_SYNC transition");
        let t = &kfm.sequences[0].transitions[0];
        assert_eq!(t.transition_type, KfmTransitionType::DefaultSync);
        assert_eq!(t.duration, 0.0);
        assert!(t.blend_pairs.is_empty());
        assert!(t.chain.is_empty());
    }

    // ── #532 / FNV-ANIM-4: KfmCatalog name + id lookup ────────────────

    fn build_catalog_two_sequences() -> KfmCatalog {
        // Reuse the 2.2.0.0 fixture above; it ships two sequences
        // (`idle.kf` id=0, `walk.kf` id=1) — enough to exercise both
        // name and id lookups plus the iter_named ordering.
        let bytes = sample_kfm_2_2_0_0();
        KfmCatalog::from_bytes(&bytes).expect("fixture parses")
    }

    #[test]
    fn catalog_finds_sequence_by_filename_stem() {
        let cat = build_catalog_two_sequences();
        let idle = cat.find_by_name("idle").expect("idle.kf stem matches");
        assert_eq!(idle.sequence_id, 0);
        assert_eq!(idle.filename, "idle.kf");

        let walk = cat.find_by_name("walk").expect("walk.kf stem matches");
        assert_eq!(walk.sequence_id, 1);
    }

    #[test]
    fn catalog_name_lookup_is_case_insensitive() {
        let cat = build_catalog_two_sequences();
        assert!(cat.find_by_name("IDLE").is_some());
        assert!(cat.find_by_name("Walk").is_some());
        assert!(cat.find_by_name("  walk  ").is_some()); // trim
    }

    #[test]
    fn catalog_strips_path_and_extension_from_filename() {
        // Construct a sequence with a Bethesda-style nested path so
        // the stem extraction has to drop both directories and ext.
        let kfm = KfmFile {
            version: (2, 2, 0, 0),
            little_endian: true,
            model_path: String::new(),
            model_root: String::new(),
            default_sync_transition: KfmTransitionDefaults::default(),
            default_nonsync_transition: KfmTransitionDefaults::default(),
            sequences: vec![
                KfmSequence {
                    sequence_id: 10,
                    filename: "characters\\_male\\idleanims\\1hmidle.kf".to_string(),
                    anim_index: -1,
                    transitions: vec![],
                },
                KfmSequence {
                    sequence_id: 11,
                    // Forward-slash path AND uppercase extension —
                    // both go through the same stem rule.
                    filename: "1stperson/runforward.KF".to_string(),
                    anim_index: -1,
                    transitions: vec![],
                },
            ],
            sequence_groups: vec![],
        };
        let cat = KfmCatalog::new(kfm);
        assert_eq!(cat.find_by_name("1hmidle").map(|s| s.sequence_id), Some(10));
        assert_eq!(
            cat.find_by_name("runforward").map(|s| s.sequence_id),
            Some(11)
        );
        // Misses must still return None — no fuzzy matching.
        assert!(cat.find_by_name("hmidle").is_none());
        assert!(cat.find_by_name("1stperson").is_none());
    }

    #[test]
    fn catalog_finds_sequence_by_id() {
        let cat = build_catalog_two_sequences();
        assert_eq!(cat.find_by_id(0).map(|s| &*s.filename), Some("idle.kf"));
        assert_eq!(cat.find_by_id(1).map(|s| &*s.filename), Some("walk.kf"));
        assert!(cat.find_by_id(99).is_none());
    }

    #[test]
    fn catalog_iter_named_yields_id_sorted_pairs() {
        // Pin the deterministic-iteration contract — `iter_named`
        // sorts by sequence_id regardless of file authoring order, so
        // the M30 actor-controller bring-up gets repeatable walks.
        let kfm = KfmFile {
            version: (2, 2, 0, 0),
            little_endian: true,
            model_path: String::new(),
            model_root: String::new(),
            default_sync_transition: KfmTransitionDefaults::default(),
            default_nonsync_transition: KfmTransitionDefaults::default(),
            sequences: vec![
                KfmSequence {
                    sequence_id: 5,
                    filename: "zeta.kf".to_string(),
                    anim_index: -1,
                    transitions: vec![],
                },
                KfmSequence {
                    sequence_id: 2,
                    filename: "alpha.kf".to_string(),
                    anim_index: -1,
                    transitions: vec![],
                },
                KfmSequence {
                    sequence_id: 8,
                    filename: "mu.kf".to_string(),
                    anim_index: -1,
                    transitions: vec![],
                },
            ],
            sequence_groups: vec![],
        };
        let cat = KfmCatalog::new(kfm);
        let order: Vec<(&str, u32)> = cat
            .iter_named()
            .map(|(n, s)| (n, s.sequence_id))
            .collect();
        assert_eq!(
            order,
            vec![("alpha", 2u32), ("zeta", 5), ("mu", 8)],
            "iter_named must sort by sequence_id, not file position"
        );
    }

    #[test]
    fn catalog_duplicate_id_keeps_first() {
        // Pathological KFM authoring — duplicate sequence_id values.
        // NiKFMTool rejects this; we keep the FIRST occurrence so a
        // malformed file degrades to "duplicate silently ignored"
        // rather than panicking on the HashMap insert.
        let kfm = KfmFile {
            version: (2, 2, 0, 0),
            little_endian: true,
            model_path: String::new(),
            model_root: String::new(),
            default_sync_transition: KfmTransitionDefaults::default(),
            default_nonsync_transition: KfmTransitionDefaults::default(),
            sequences: vec![
                KfmSequence {
                    sequence_id: 7,
                    filename: "first.kf".to_string(),
                    anim_index: -1,
                    transitions: vec![],
                },
                KfmSequence {
                    sequence_id: 7,
                    filename: "second.kf".to_string(),
                    anim_index: -1,
                    transitions: vec![],
                },
            ],
            sequence_groups: vec![],
        };
        let cat = KfmCatalog::new(kfm);
        assert_eq!(
            cat.find_by_id(7).map(|s| &*s.filename),
            Some("first.kf"),
            "duplicate sequence_id must keep the first occurrence"
        );
    }
}
