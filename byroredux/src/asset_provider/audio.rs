//! FormID → archive-relative audio path resolution, plus REGN ambient
//! background-music dispatch.
//!
//! EX-16 item 1 (#2372) named the FormID→path gap as the missing half of
//! REGN ambient-sound consumption: `RegionDataKind::Sound` carries a
//! `sound_form: u32` pointing at a `SOUN` record, but until
//! `soun::parse_soun` there was no decoded file path to resolve it to.
//! Item 5 is the other half — actually playing something. This module now
//! covers both: [`resolve_sound_path`]/[`sound_archive_path`] are the
//! pure lookup/normalization pair (mirroring `script::pex_archive_path`'s
//! shape for Papyrus `.pex` names), and [`SoundArchiveProvider`] +
//! [`dispatch_region_ambient_music`] are the archive-backed dispatch:
//! resolve `RegionAmbientRes::music_form` → archive key → extracted bytes
//! → a streaming `AudioWorld::play_music` call, or a `stop_music` when
//! there's nothing to play.
//!
//! Deliberately still NOT covered: the `incidental`/`sounds` ambient-loop
//! fields, or a spatial REGN-keyed `AudioEmitter`. `music` is the only
//! REGN field with unambiguous, chance-free semantics (a single
//! background-track FormID); `incidental`/`sounds` selection needs design
//! work this session doesn't attempt (see `RegionAmbientRes`'s doc).

use super::*;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{first_playable_track, EsmIndex, MuscRecord, MustRecord, SounRecord};
use std::collections::HashMap;

/// The record tables REGN background music resolves through: `SOUN` for the eras whose `music_form`
/// is a sound, and Skyrim's `MUSC` → `MUST` chain for `RDMO` (#3816).
#[derive(Clone, Copy)]
pub(crate) struct RegionMusicTables<'a> {
    pub(crate) sounds: &'a HashMap<u32, SounRecord>,
    pub(crate) music_types: &'a HashMap<u32, MuscRecord>,
    pub(crate) music_tracks: &'a HashMap<u32, MustRecord>,
}

impl<'a> RegionMusicTables<'a> {
    pub(crate) fn new(index: &'a EsmIndex) -> Self {
        Self { sounds: &index.sounds, music_types: &index.music_types, music_tracks: &index.music_tracks }
    }
}

/// Archive key of a Skyrim `MUST.ANAM` track: lower-case, `\\`-separated, `music\\`-rooted.
///
/// Vanilla `Skyrim.esm` authors it from the game root with a leading separator
/// (`\\Data\\Music\\Explore\\….wav`, all 28 region music types); other records are data-relative
/// (`Music\\…`) or music-relative. Either way the archive entry is `music\\<...>`.
pub(crate) fn music_archive_path(file: &str) -> String {
    let normalized = file.replace('/', "\\").to_ascii_lowercase();
    let mut path = normalized.trim_start_matches('\\').to_owned();
    if let Some(rest) = path.strip_prefix("data\\") {
        path = rest.to_owned();
    }
    if !path.starts_with("music\\") {
        path = format!("music\\{path}");
    }
    path
}

/// Skyrim `RDMO` → `MUSC` → first playable `MUST` track, as an archive key. `None` when the form is
/// not a `MUSC` or none of its tracks is playable (all silent, empty, or dangling).
pub(crate) fn resolve_music_track_path(tables: &RegionMusicTables<'_>, music_form: u32) -> Option<String> {
    let musc = tables.music_types.get(&music_form)?;
    let track = first_playable_track(musc, tables.music_tracks)?;
    Some(music_archive_path(&track.file))
}

/// The `.xwm` entry Skyrim ships for a track its records author as `.wav`: every one of the 28 vanilla
/// region music tracks is authored `….wav` while `Skyrim - Sounds.bsa` holds only `….xwm`, the engine
/// substituting the extension. `None` for a key that is not `.wav`.
pub(crate) fn xwma_sibling(archive_path: &str) -> Option<String> {
    archive_path.strip_suffix(".wav").map(|stem| format!("{stem}.xwm"))
}

/// Whether an archive key names xWMA audio, which the streaming loader cannot decode: the audio
/// crate's xWMA route is `.fuz`-only and shells out to a host `ffmpeg` (#5912), too slow to run
/// synchronously for a multi-minute track at cell load. 265 of Skyrim's 282 music tracks are `.xwm`.
pub(crate) fn is_xwma_path(archive_path: &str) -> bool {
    archive_path.ends_with(".xwm") || archive_path.ends_with(".xwma")
}

/// Look up a `SOUN` FormID's decoded `FNAM` path, verbatim. Returns `None`
/// when the FormID isn't a known SOUN (bad data) or the record omitted
/// `FNAM` (rare placeholder records — see `soun::parse_soun`'s doc).
///
/// #3914 — the path is a **file or a folder** (`SounRecord::is_folder`;
/// 50.8 % of FNV's `SOUN` records author a folder of variants). Callers
/// must check [`sound_is_folder`] before handing the result to
/// [`sound_archive_path`] + [`SoundArchiveProvider::extract`]: a folder is
/// never an archive entry, so extracting it is a guaranteed miss that
/// looks exactly like missing content.
pub(crate) fn resolve_sound_path(sounds: &HashMap<u32, SounRecord>, form_id: u32) -> Option<&str> {
    sounds
        .get(&form_id)
        .map(|s| s.sound_path.as_str())
        .filter(|p| !p.is_empty())
}

/// Whether a `SOUN` FormID's `FNAM` names a folder of variant files rather
/// than one file (#3914; see [`SounRecord::is_folder`]). `false` for an
/// unresolved FormID, same fail-closed posture as [`resolve_sound_path`] —
/// kept as its own pure lookup, mirroring [`sound_loops`].
pub(crate) fn sound_is_folder(sounds: &HashMap<u32, SounRecord>, form_id: u32) -> bool {
    sounds.get(&form_id).is_some_and(|s| s.is_folder())
}

/// Look up a `SOUN` FormID's [`SounRecord::looping`] flag. `false` for an
/// unresolved FormID, same fail-closed posture as [`resolve_sound_path`].
/// #3775 — kept as its own pure lookup (mirroring `resolve_sound_path`'s
/// shape) rather than folded into a combined return, so both stay
/// independently unit-testable without a `World`/`AudioWorld`.
pub(crate) fn sound_loops(sounds: &HashMap<u32, SounRecord>, form_id: u32) -> bool {
    sounds.get(&form_id).is_some_and(|s| s.looping)
}

/// Normalise a `SOUN.FNAM` value to its archive form: lowercase,
/// backslash-separated, under the `sound\` folder. `FNAM` is authored
/// relative to `Data\Sound\` without that prefix, the way `MODL` is
/// relative to `Meshes\` and `ICON` to `Textures\` — but unlike those,
/// `FNAM` is not always a file (#3914): a folder-form value keeps its
/// trailing separator here (`fx\amb\ceilingcrumble\` →
/// `sound\fx\amb\ceilingcrumble\`), which is an entry *prefix*, never an
/// entry key — only a file-form result is valid input to
/// [`SoundArchiveProvider::extract`]. A path that already carries the
/// folder, or uses forward slashes, is accepted unchanged in meaning.
/// Mirrors `script::pex_archive_path`.
pub(crate) fn sound_archive_path(sound_path: &str) -> String {
    let mut path = sound_path.replace('/', "\\").to_ascii_lowercase();
    // Some records author the path from the game root (`Data\Sound\FX\OBJ\PullChain\…`,
    // Skyrim.esm's pull-chain descriptors); the archive key is `sound\fx\…`.
    if let Some(rest) = path.strip_prefix("data\\") {
        path = rest.to_owned();
    }
    if !path.starts_with("sound\\") {
        path = format!("sound\\{path}");
    }
    path
}

/// Searches game archives for sound files by archive-relative path.
/// `--sounds-bsa` is repeatable — LAST-listed wins (#3917 flipped the
/// lookup from #3776's first-wins; list override/mod archives after the
/// vanilla one), the opposite precedence of `ScriptProvider`. An
/// empty provider (flag absent) makes every lookup a clean miss so callers
/// fall through to "nothing to play" the same way an unregistered SOUN
/// would.
///
/// This is a different animal from the ad hoc `Archive::open` calls in
/// `try_load_default_footstep`/`try_load_default_water_splash`: those each
/// resolve exactly one hardcoded canonical path once at boot, so reopening
/// the archive on the spot is fine. REGN ambient dispatch resolves an
/// arbitrary FormID-driven path on every cell/tile change for the whole
/// engine session, which needs a persistent handle — the same reason
/// `ScriptProvider` exists instead of every VMAD attach reopening
/// `--scripts-bsa` from scratch.
pub(crate) struct SoundArchiveProvider {
    archives: Vec<Archive>,
}

impl SoundArchiveProvider {
    pub(crate) fn new() -> Self {
        Self {
            archives: Vec::new(),
        }
    }

    /// True when no sound archive was supplied — dispatch can skip the
    /// extract attempt entirely.
    pub(crate) fn is_empty(&self) -> bool {
        self.archives.is_empty()
    }

    /// Extract raw bytes for an archive-relative **file** path (as produced
    /// by [`sound_archive_path`] from a file-form `FNAM`).
    ///
    /// **Precedence: last-listed archive wins**, matching Bethesda load order
    /// and the mesh / texture / material pools #3637 (`3562401b`) inverted.
    /// Sound archives are plain content archives like those three, so they
    /// follow the same rule; `ScriptProvider` is first-wins deliberately and
    /// for a different reason (`--scripts-bsa` carries a mod principal and
    /// override archives are listed first — #1743 / SCR-D7-03), which is why
    /// that one says so in its own doc.
    ///
    /// #3917 — this provider was missed by #3637's sweep and stayed
    /// first-wins. That was inert while the FNV profile listed one sound
    /// archive, but `[profiles.fnv]` now lists
    /// `["Fallout - Sound.bsa", "Update.bsa"]` with a comment stating the
    /// patch goes last "under last-wins" — which this function was not
    /// honouring. Measured on the shipped archives: the two overlap on
    /// exactly one key, `sound\fx\wpn\minigun\wpn_minigun_spin_lpm.wav`,
    /// and the copies differ (267 774 B in `Fallout - Sound.bsa` vs 210 534 B
    /// in `Update.bsa`), so first-wins really was serving the unpatched
    /// audio. `--game skyrim_se` supplies two sound archives as well.
    ///
    /// A folder-form path (#3914) is never an entry key — it always misses
    /// here, so callers gate on [`sound_is_folder`] first rather than reading
    /// that miss as missing content. Variant selection inside a folder is a
    /// policy decision no consumer has made yet, so there is deliberately no
    /// `extract_any_in(folder)` sibling.
    pub(crate) fn extract(&self, archive_path: &str) -> Option<Vec<u8>> {
        extract_first(self.archives.iter().rev(), archive_path)
    }

    /// Whether any supplied archive holds `archive_path` — an index lookup, no extraction.
    pub(crate) fn contains(&self, archive_path: &str) -> bool {
        self.archives.iter().any(|archive| archive.contains(archive_path))
    }
}

impl Resource for SoundArchiveProvider {}

/// Build a [`SoundArchiveProvider`] from CLI arguments. Accepts repeated
/// `--sounds-bsa <path>` flags (the same flag `try_load_default_footstep`/
/// `try_load_default_water_splash` already consume for their one-off
/// canonical-path loads — opening the archives here again is cheap:
/// `BsaArchive`/`Ba2Archive::open` reads only the file table, not the
/// payload). Silently returns an empty provider when no flag is present.
pub(crate) fn build_sound_archive_provider(args: &[String]) -> SoundArchiveProvider {
    let mut provider = SoundArchiveProvider::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--sounds-bsa" {
            if let Some(path) = args.get(i + 1) {
                match Archive::open(path) {
                    Ok(a) => {
                        log::info!("Opened sound archive: '{path}'");
                        provider.archives.push(a);
                    }
                    Err(e) => log::warn!("Failed to open sound archive '{path}': {e}"),
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    provider
}

/// Ambient-track crossfade duration. Long enough that the outgoing and
/// incoming beds overlap smoothly rather than butting into each other;
/// short enough that walking briskly between two differently-scored
/// regions doesn't leave a stale track audible for several seconds after
/// the transition. `AudioWorld::play_music` applies this symmetrically —
/// fade the old track out, fade the new one in, over the same window.
const REGN_AMBIENT_CROSSFADE_SECS: f32 = 3.0;

/// Nominal playback volume for REGN ambient tracks (linear amplitude —
/// `AudioWorld::play_music`'s own scale, 1.0 = authored level). No
/// per-region volume field exists in the parsed `RDAT` data to scale by.
const REGN_AMBIENT_VOLUME: f32 = 1.0;

/// Dispatch the REGN ambient background track for a freshly resolved
/// [`crate::components::RegionAmbientRes`] — called only when
/// `music_form` actually changed from whatever was previously installed
/// (both call sites compare against the resource's prior value before
/// calling this; see `cell_loader::load::load_cell_with_masters` and
/// `scene::apply_cell_region_ambient`). An unconditional redispatch on
/// every cell load/crossing would restart — with an audible crossfade —
/// the exact same track any time two connected cells/tiles share one
/// tagging region.
///
/// No-ops (or stops the current track, see below) cleanly through every
/// missing layer: no `SoundArchiveProvider` resource, no `--sounds-bsa`
/// archive supplied, the FormID doesn't resolve to a SOUN with a path,
/// the archive doesn't carry the file, or decode fails. On any of those
/// failures — as well as on `music_form: None` — outstanding REGN
/// ambient playback is stopped rather than left running: whatever was
/// audible belonged to the *previous* cell's directive, and continuing
/// to play it into a cell that doesn't call for it (or calls for
/// something this engine build can't load) is worse than silence.
pub(crate) fn dispatch_region_ambient_music(
    world: &mut World,
    tables: RegionMusicTables<'_>,
    music_form: Option<u32>,
) {
    let sounds = tables.sounds;
    let resolved = music_form.and_then(|form_id| resolve_sound_path(sounds, form_id));
    // #3816 — Skyrim's `RDMO` is a `MUSC`, whose first playable `MUST` track names the file. A FormID
    // is one record, so this and the `SOUN` lookup above can never both hit.
    let music_track = match resolved {
        Some(_) => None,
        None => music_form.and_then(|form_id| resolve_music_track_path(&tables, form_id)),
    };
    // #3775 — whether the engine should loop this track continuously.
    let looping = music_form.is_some_and(|form_id| sound_loops(sounds, form_id));
    // #3787 (FNV) / #3811 (Oblivion + Skyrim) — `music_form` was authored
    // (a real REGN chose an ambient directive) but never resolves as a
    // `SOUN` on any game: Oblivion's `RDMD` is a music-category enum, not
    // a FormID at all; Skyrim's `RDMO` targets `MUSC`; FNV's `RDSB`/`RDSI`
    // target `MSET` (Media Set). FO3/FNV `MUSC` is decoded
    // (`misc/music.rs`, consumed by scripted `PlayMusic`), but no REGN-driven
    // runtime exists for Skyrim `MUSC`/`MUST` or FNV `MSET` (#3816 tracks them
    // — #3787 and #3811 closed doc-only, and #3915 folded FNV's `MSET` into
    // #3816; `MSET` is decoded into `EsmIndex::media_sets`, but no playback
    // policy reads it yet), so this path is structurally unsupported on every game
    // rather than a content gap; log it once so "no archive supplied" (silent,
    // the common case per the doc above) is distinguishable from "an
    // ambient directive was authored but this engine build can't resolve
    // its target type at all" — repeated per-region-transition logging
    // would otherwise flood the log with the same diagnosis every cell
    // load.
    if music_form.is_some() && resolved.is_none() && music_track.is_none() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            log::info!(
                "REGN ambient: music_form resolved to neither a SOUN nor a playable \
                 Skyrim MUSC track — Oblivion's RDMD is a music-category enum (not a \
                 FormID) and FNV's RDSB/RDSI target MSET (Media Set); neither has a \
                 playback runtime yet (#3816), and a Skyrim MUSC with no playable MUST \
                 track is likewise silent. Not a missing-archive content gap"
            );
        });
    }
    let archive_path = match (resolved, &music_track) {
        (Some(path), _) => Some(sound_archive_path(path)),
        (None, Some(track)) => Some(track.clone()),
        (None, None) => None,
    };
    let Some(mut archive_path) = archive_path else {
        stop_region_ambient_music(world);
        return;
    };
    // #3816 — a Skyrim track authored `.wav` ships as `.xwm`; prefer the authored name if the archive
    // has it (the 17 real `.wav` tracks), else its xWMA sibling.
    if music_track.is_some() {
        if let (Some(provider), Some(sibling)) =
            (world.try_resource::<SoundArchiveProvider>(), xwma_sibling(&archive_path))
        {
            if !provider.contains(&archive_path) && provider.contains(&sibling) {
                archive_path = sibling;
            }
        }
    }
    // #3816 — Skyrim's music is xWMA; see `is_xwma_path`. Say what is missing instead of letting the
    // decode below fail and blame the archive.
    if is_xwma_path(&archive_path) {
        static XWMA_ONCE: std::sync::Once = std::sync::Once::new();
        XWMA_ONCE.call_once(|| {
            log::info!(
                "REGN ambient: '{archive_path}' is xWMA — the streaming music path has no xWMA \
                 decoder (the .fuz route needs a host ffmpeg), so Skyrim region music stays silent \
                 until one exists (#3816)"
            );
        });
        stop_region_ambient_music(world);
        return;
    }
    // #3914 — a folder-form `FNAM` (half of FNV's SOUN library) names a
    // set of variants to pick from, not an entry; extracting it would be
    // a guaranteed miss logged as "not found in any archive", which is a
    // different diagnosis (missing content) from the truth (a selection
    // policy this engine hasn't implemented). Fail closed, say so.
    if music_form.is_some_and(|form_id| sound_is_folder(sounds, form_id)) {
        log::warn!(
            "REGN ambient: '{archive_path}' is a folder of variants (SOUN.FNAM folder \
             form) — variant selection is not implemented, track skipped (#3914)"
        );
        stop_region_ambient_music(world);
        return;
    }

    // Scoped so the `SoundArchiveProvider` read guard drops before any
    // `&mut World` use below — the guard's `Drop` impl otherwise keeps the
    // immutable borrow alive past every early-return inside a shared block
    // (E0502). `provider_present` distinguishes "no --sounds-bsa supplied"
    // (silent no-op, the common case) from "supplied, but this file isn't
    // in it" (worth a warning — a real content gap).
    let (provider_present, bytes) = {
        match world.try_resource::<SoundArchiveProvider>() {
            Some(p) if !p.is_empty() => (true, p.extract(&archive_path)),
            _ => (false, None),
        }
    };
    let Some(bytes) = bytes else {
        if provider_present {
            log::warn!("REGN ambient: '{archive_path}' not found in any --sounds-bsa archive");
        }
        stop_region_ambient_music(world);
        return;
    };

    let streaming = match byroredux_audio::load_streaming_sound_from_bytes(bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("REGN ambient: decode '{archive_path}' failed: {e}");
            stop_region_ambient_music(world);
            return;
        }
    };

    let Some(mut audio_world) = world.try_resource_mut::<byroredux_audio::AudioWorld>() else {
        return;
    };
    log::info!(
        "REGN ambient: playing '{archive_path}'{}",
        if looping { " (looping)" } else { "" }
    );
    audio_world.play_music(
        streaming,
        REGN_AMBIENT_VOLUME,
        REGN_AMBIENT_CROSSFADE_SECS,
        looping,
    );
}

fn stop_region_ambient_music(world: &mut World) {
    if let Some(mut audio_world) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
        audio_world.stop_music(REGN_AMBIENT_CROSSFADE_SECS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tables with only `SOUN`s populated: the pre-#3816 shape every older test dispatches with.
    fn sounds_only(sounds: &HashMap<u32, SounRecord>) -> RegionMusicTables<'_> {
        static EMPTY_TYPES: std::sync::OnceLock<HashMap<u32, MuscRecord>> = std::sync::OnceLock::new();
        static EMPTY_TRACKS: std::sync::OnceLock<HashMap<u32, MustRecord>> = std::sync::OnceLock::new();
        RegionMusicTables {
            sounds,
            music_types: EMPTY_TYPES.get_or_init(HashMap::new),
            music_tracks: EMPTY_TRACKS.get_or_init(HashMap::new),
        }
    }

    fn soun(form_id: u32, path: &str) -> SounRecord {
        SounRecord {
            form_id,
            editor_id: String::new(),
            sound_path: path.to_string(),
            looping: false,
            variants: Vec::new(),
            ..Default::default()
        }
    }

    fn looping_soun(form_id: u32, path: &str) -> SounRecord {
        SounRecord {
            looping: true,
            ..soun(form_id, path)
        }
    }

    #[test]
    fn resolve_sound_path_finds_known_form_id() {
        let mut sounds = HashMap::new();
        sounds.insert(0x1234, soun(0x1234, "fx\\explosion01.wav"));
        assert_eq!(
            resolve_sound_path(&sounds, 0x1234),
            Some("fx\\explosion01.wav")
        );
    }

    #[test]
    fn resolve_sound_path_missing_form_id_returns_none() {
        let sounds = HashMap::new();
        assert_eq!(resolve_sound_path(&sounds, 0xDEAD_BEEF), None);
    }

    #[test]
    fn resolve_sound_path_empty_fnam_returns_none() {
        // Placeholder SOUN with no FNAM decodes to an empty path — treat
        // it the same as "not found" rather than handing callers an
        // archive lookup that can never hit.
        let mut sounds = HashMap::new();
        sounds.insert(0x5, soun(0x5, ""));
        assert_eq!(resolve_sound_path(&sounds, 0x5), None);
    }

    #[test]
    fn sound_loops_true_for_a_looping_record() {
        let mut sounds = HashMap::new();
        sounds.insert(0x1234, looping_soun(0x1234, "amb\\wind_loop.wav"));
        assert!(sound_loops(&sounds, 0x1234));
    }

    #[test]
    fn sound_loops_false_for_a_non_looping_record() {
        let mut sounds = HashMap::new();
        sounds.insert(0x1234, soun(0x1234, "fx\\explosion01.wav"));
        assert!(!sound_loops(&sounds, 0x1234));
    }

    #[test]
    fn sound_loops_false_for_an_unresolved_form_id() {
        let sounds = HashMap::new();
        assert!(!sound_loops(&sounds, 0xDEAD_BEEF));
    }

    /// #3914 — the folder form is carried through the FormID lookup
    /// unchanged, and the sibling predicate reports it, so a consumer can
    /// branch before it ever reaches `extract`.
    #[test]
    fn sound_is_folder_true_for_a_folder_form_record() {
        let mut sounds = HashMap::new();
        sounds.insert(0x77, soun(0x77, "fx\\amb\\ceilingcrumble\\"));
        assert!(sound_is_folder(&sounds, 0x77));
        assert_eq!(
            resolve_sound_path(&sounds, 0x77),
            Some("fx\\amb\\ceilingcrumble\\"),
            "the folder path itself still resolves — only its kind differs"
        );
    }

    #[test]
    fn sound_is_folder_false_for_a_file_form_record_and_unknown_form_id() {
        let mut sounds = HashMap::new();
        sounds.insert(0x1234, soun(0x1234, "fx\\explosion01.wav"));
        assert!(!sound_is_folder(&sounds, 0x1234));
        assert!(!sound_is_folder(&sounds, 0xDEAD_BEEF));
    }

    /// Skyrim.esm authors the pull-chain descriptors from the game root
    /// (`Data\Sound\FX\OBJ\PullChain\…`); the archive key has neither prefix.
    #[test]
    fn sound_archive_path_drops_a_game_root_prefix() {
        assert_eq!(
            sound_archive_path("Data\\Sound\\FX\\OBJ\\PullChain\\OBJ_PullChain_Down_01.wav"),
            "sound\\fx\\obj\\pullchain\\obj_pullchain_down_01.wav"
        );
        assert_eq!(sound_archive_path("fx\\a.wav"), "sound\\fx\\a.wav");
    }

    /// #3914 — a folder-form value keeps its trailing separator through
    /// normalisation: the result is an entry prefix, never an entry key.
    #[test]
    fn sound_archive_path_keeps_folder_form_trailing_separator() {
        assert_eq!(
            sound_archive_path("FX\\Amb\\CeilingCrumble\\"),
            "sound\\fx\\amb\\ceilingcrumble\\"
        );
        assert_eq!(
            sound_archive_path("fx/amb/ceilingcrumble/"),
            "sound\\fx\\amb\\ceilingcrumble\\"
        );
    }

    #[test]
    fn sound_archive_path_prepends_folder_and_lowercases() {
        assert_eq!(
            sound_archive_path("FX\\Explosion01.wav"),
            "sound\\fx\\explosion01.wav"
        );
    }

    #[test]
    fn sound_archive_path_normalizes_forward_slashes() {
        assert_eq!(
            sound_archive_path("amb/wind_loop.wav"),
            "sound\\amb\\wind_loop.wav"
        );
    }

    #[test]
    fn sound_archive_path_does_not_double_prefix() {
        // Defensive: a path already authored with the `sound\` folder
        // (non-vanilla authoring) must not become `sound\sound\...`.
        assert_eq!(
            sound_archive_path("sound\\fx\\explosion01.wav"),
            "sound\\fx\\explosion01.wav"
        );
    }

    #[test]
    fn sound_archive_provider_starts_empty() {
        let provider = SoundArchiveProvider::new();
        assert!(provider.is_empty());
        assert!(provider.extract("sound\\fx\\explosion01.wav").is_none());
    }

    #[test]
    fn build_sound_archive_provider_without_flag_is_empty() {
        let provider = build_sound_archive_provider(&["byroredux".to_string()]);
        assert!(provider.is_empty());
    }

    #[test]
    fn build_sound_archive_provider_with_unopenable_path_stays_empty() {
        // A bad/missing path must warn and skip, not panic the whole boot.
        let provider = build_sound_archive_provider(&[
            "--sounds-bsa".to_string(),
            "/nonexistent/path/does-not-exist.bsa".to_string(),
        ]);
        assert!(provider.is_empty());
    }

    /// `dispatch_region_ambient_music` must survive every layer being
    /// absent: no `SoundArchiveProvider` resource registered at all.
    /// Device/headless-safe — no real audio manager needed since the
    /// function returns before ever touching `AudioWorld`.
    #[test]
    fn dispatch_with_no_provider_resource_does_not_panic() {
        let mut world = World::new();
        let sounds = HashMap::new();
        dispatch_region_ambient_music(&mut world, sounds_only(&sounds), Some(0x1234));
    }

    /// An empty provider (boot without `--sounds-bsa`) must no-op the same
    /// way, including stopping any prior track — verified via absence of
    /// `AudioWorld` here too (the no-op path never reaches it when
    /// `AudioWorld` isn't registered).
    #[test]
    fn dispatch_with_empty_provider_does_not_panic() {
        let mut world = World::new();
        world.insert_resource(SoundArchiveProvider::new());
        let sounds = HashMap::new();
        dispatch_region_ambient_music(&mut world, sounds_only(&sounds), Some(0x1234));
    }

    /// `music_form: None` (no REGN directive, or the winning entry omits
    /// `music`) must stop playback rather than error — exercised against an
    /// explicitly headless `AudioWorld` so `stop_music`'s no-op path is reached.
    #[test]
    fn dispatch_with_no_music_form_stops_playback_without_panic() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        let sounds = HashMap::new();
        dispatch_region_ambient_music(&mut world, sounds_only(&sounds), None);
        assert!(!world
            .resource::<byroredux_audio::AudioWorld>()
            .is_music_active());
    }

    /// #3914 — a `music_form` whose SOUN is folder-form must fail closed
    /// (stop playback) *before* any archive lookup, so the folder is never
    /// mistaken for a missing file. Exercised with an explicitly headless
    /// `AudioWorld` and a registered-but-empty provider so every layer the
    /// folder branch precedes is actually present.
    #[test]
    fn dispatch_with_folder_form_soun_stops_playback_without_archive_lookup() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        world.insert_resource(SoundArchiveProvider::new());
        let mut sounds = HashMap::new();
        sounds.insert(0x77, soun(0x77, "fx\\amb\\ceilingcrumble\\"));
        dispatch_region_ambient_music(&mut world, sounds_only(&sounds), Some(0x77));
        assert!(!world
            .resource::<byroredux_audio::AudioWorld>()
            .is_music_active());
    }

    /// A `music_form` that doesn't resolve to any known SOUN must stop
    /// playback (not leave a stale track running) rather than panic.
    ///
    /// #3787 — this is the exact shape of FNV's `RDSB`/`RDSI` fields
    /// (census-confirmed: 44/44 + 10/11 targets are `MSET`, not `SOUN`,
    /// so the FormID is real but absent from the `sounds` map every
    /// time). Pins that a real-but-non-SOUN FormID does not get spuriously
    /// resolved through the SOUN map — it fails closed exactly like a
    /// genuinely unknown FormID does, never fabricating a lookup hit.
    #[test]
    fn dispatch_with_unresolvable_form_id_stops_playback() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        world.insert_resource(SoundArchiveProvider::new());
        let sounds = HashMap::new();
        // 0xDEAD_BEEF stands in for a real MSET FormID here: authored
        // (Some), present in no SOUN map, same as every FNV RDSB target.
        dispatch_region_ambient_music(&mut world, sounds_only(&sounds), Some(0xDEAD_BEEF));
        assert!(!world
            .resource::<byroredux_audio::AudioWorld>()
            .is_music_active());
    }

    fn skyrim_tables() -> (HashMap<u32, MuscRecord>, HashMap<u32, MustRecord>) {
        use byroredux_plugin::esm::records::MustKind;
        let musc = MuscRecord { form_id: 0x10, tracks: vec![0x20], ..Default::default() };
        let palette = MustRecord { form_id: 0x20, kind: MustKind::Palette, sub_tracks: vec![0x21], ..Default::default() };
        let single = MustRecord {
            form_id: 0x21,
            kind: MustKind::SingleTrack,
            file: "Music\\Explore\\Tundra_A.xwm".into(),
            ..Default::default()
        };
        (
            [(0x10, musc)].into_iter().collect(),
            [(0x20, palette), (0x21, single)].into_iter().collect(),
        )
    }

    /// #3816 — a Skyrim `RDMO` resolves MUSC → palette → first `SingleTrack` to a `music\\` archive key.
    #[test]
    fn a_skyrim_music_type_resolves_through_its_palette_to_an_archive_key() {
        let (types, tracks) = skyrim_tables();
        let sounds = HashMap::new();
        let tables = RegionMusicTables { sounds: &sounds, music_types: &types, music_tracks: &tracks };
        assert_eq!(
            resolve_music_track_path(&tables, 0x10).as_deref(),
            Some("music\\explore\\tundra_a.xwm")
        );
        assert_eq!(resolve_music_track_path(&tables, 0x99), None, "not a MUSC");
    }

    #[test]
    fn music_archive_path_roots_at_music_without_doubling() {
        assert_eq!(music_archive_path("Explore/A.xwm"), "music\\explore\\a.xwm");
        assert_eq!(music_archive_path("Music\\Explore\\A.xwm"), "music\\explore\\a.xwm");
        assert_eq!(music_archive_path("Data\\Music\\A.wav"), "music\\a.wav");
        // The vanilla Skyrim.esm shape: from the game root, with a leading separator.
        assert_eq!(
            music_archive_path("\\Data\\Music\\Explore\\MUS_Explore_Day_01.wav"),
            "music\\explore\\mus_explore_day_01.wav"
        );
    }

    #[test]
    fn a_wav_authored_track_has_an_xwm_sibling() {
        assert_eq!(xwma_sibling("music\\explore\\a.wav").as_deref(), Some("music\\explore\\a.xwm"));
        assert_eq!(xwma_sibling("music\\explore\\a.xwm"), None);
    }

    #[test]
    fn xwma_music_is_fenced_off_from_the_streaming_decoder() {
        assert!(is_xwma_path("music\\explore\\a.xwm"));
        assert!(!is_xwma_path("music\\special\\a.wav"));
    }

    /// A Skyrim MUSC with an `.xwm` track reaches the xWMA fence and stops playback — no panic, no
    /// archive read — even with every other layer absent.
    #[test]
    fn dispatching_a_skyrim_xwm_music_type_stops_without_panic() {
        let (types, tracks) = skyrim_tables();
        let sounds = HashMap::new();
        let tables = RegionMusicTables { sounds: &sounds, music_types: &types, music_tracks: &tracks };
        let mut world = World::new();
        dispatch_region_ambient_music(&mut world, tables, Some(0x10));
    }
}
