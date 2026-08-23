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
use byroredux_plugin::esm::records::SounRecord;
use std::collections::HashMap;

/// Look up a `SOUN` FormID's decoded file path. Returns `None` when the
/// FormID isn't a known SOUN (bad data) or the record omitted `FNAM`
/// (rare placeholder records — see `soun::parse_soun`'s doc).
pub(crate) fn resolve_sound_path(sounds: &HashMap<u32, SounRecord>, form_id: u32) -> Option<&str> {
    sounds
        .get(&form_id)
        .map(|s| s.sound_path.as_str())
        .filter(|p| !p.is_empty())
}

/// Normalise a `SOUN.FNAM` value to its archive key: lowercase,
/// backslash-separated, under the `sound\` folder. `FNAM` is authored
/// relative to `Data\Sound\` without that prefix (the same convention as
/// `MODL` being relative to `Meshes\` and `ICON` to `Textures\`); a path
/// that already carries the folder, or uses forward slashes, is accepted
/// unchanged in meaning. Mirrors `script::pex_archive_path`.
pub(crate) fn sound_archive_path(sound_path: &str) -> String {
    let mut path = sound_path.replace('/', "\\").to_ascii_lowercase();
    if !path.starts_with("sound\\") {
        path = format!("sound\\{path}");
    }
    path
}

/// Searches game archives for sound files by archive-relative path. Mirrors
/// `ScriptProvider`'s shape exactly: `--sounds-bsa` is repeatable (list
/// override/mod archives before the vanilla one — first hit wins), and an
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

    /// Extract raw bytes for an archive-relative path (as produced by
    /// [`sound_archive_path`]). First-listed archive wins on a collision.
    pub(crate) fn extract(&self, archive_path: &str) -> Option<Vec<u8>> {
        for archive in &self.archives {
            if let Ok(data) = archive.extract(archive_path) {
                return Some(data);
            }
        }
        None
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
    sounds: &HashMap<u32, SounRecord>,
    music_form: Option<u32>,
) {
    let archive_path = music_form
        .and_then(|form_id| resolve_sound_path(sounds, form_id))
        .map(sound_archive_path);
    let Some(archive_path) = archive_path else {
        stop_region_ambient_music(world);
        return;
    };

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
    log::info!("REGN ambient: playing '{archive_path}'");
    audio_world.play_music(streaming, REGN_AMBIENT_VOLUME, REGN_AMBIENT_CROSSFADE_SECS);
}

fn stop_region_ambient_music(world: &mut World) {
    if let Some(mut audio_world) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
        audio_world.stop_music(REGN_AMBIENT_CROSSFADE_SECS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soun(form_id: u32, path: &str) -> SounRecord {
        SounRecord {
            form_id,
            editor_id: String::new(),
            sound_path: path.to_string(),
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
        dispatch_region_ambient_music(&mut world, &sounds, Some(0x1234));
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
        dispatch_region_ambient_music(&mut world, &sounds, Some(0x1234));
    }

    /// `music_form: None` (no REGN directive, or the winning entry omits
    /// `music`) must stop playback rather than error — exercised against a
    /// real (headless-fallback) `AudioWorld` so `stop_music`'s no-op path
    /// is actually reached.
    #[test]
    fn dispatch_with_no_music_form_stops_playback_without_panic() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        let sounds = HashMap::new();
        dispatch_region_ambient_music(&mut world, &sounds, None);
        assert!(!world
            .resource::<byroredux_audio::AudioWorld>()
            .is_music_active());
    }

    /// A `music_form` that doesn't resolve to any known SOUN must stop
    /// playback (not leave a stale track running) rather than panic.
    #[test]
    fn dispatch_with_unresolvable_form_id_stops_playback() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        world.insert_resource(SoundArchiveProvider::new());
        let sounds = HashMap::new();
        dispatch_region_ambient_music(&mut world, &sounds, Some(0xDEAD_BEEF));
        assert!(!world
            .resource::<byroredux_audio::AudioWorld>()
            .is_music_active());
    }
}
