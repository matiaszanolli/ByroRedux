//! #5367 Phase V — per-line dialogue voice playback.
//!
//! Bethesda's authored convention (pinned against FNV's
//! `Fallout - Voices1.bsa` and FO3's `Fallout - Voices.bsa`):
//! `sound\voice\<plugin file>\<voice-type EDID>\<quest>_<topic>_<INFO
//! local id 8-hex>_<response>.ogg` — e.g. Doc Mitchell's greeting (INFO
//! 0x00107222, quest `VCG01`, topic `GREETING`) lives at
//! `sound\voice\falloutnv.esm\maleuniquedocmitchell\vcg01_greeting_00107222_1.ogg`.
//!
//! - **Truncated EDIDs (#5395).** The quest EDID is cut to 10 characters
//!   and the topic EDID so the two total 25 (`VDoctors` +
//!   `DoctorMedical99YES` → `vdoctors_doctormedical99ye`). Radio quests
//!   (RadioNewVegas, GNR) keep their full EDIDs, so the untruncated shape
//!   stays second in the try-order. Measured (LC-D3-01): the full shape
//!   alone resolved 28% of FNV's 52,876 joinable lines.
//! - **Plugin-local id (#5395).** The FormID is written with the
//!   load-order byte stripped (top byte `00`), DLC archives included —
//!   the id [`GlobalFormIdResolver`] decomposes, not the global FormID.
//! - **Owning plugin (#5393).** The plugin segment is the basename of the
//!   plugin that owns the INFO, resolved through the session's
//!   [`GlobalFormIdResolver`] — masters own the low slots, so the FormID's
//!   top byte is not an index into `--master` / `--esm`.
//! - **Response numbers (#5395).** Each segment's authored
//!   `response_number` names its file; numbering does not always run
//!   1..k (315 FNV / 19 FO3 INFOs skip).
//!
//! INFO records routinely carry **no EDID of their own**, so the name is
//! composed from the owning quest's and topic's EDIDs; a quest-less line
//! has no authored shape (the bare `<formid>_<n>.ogg` form matched zero
//! files in either archive) and stays silent. The voice-type EDID
//! resolves through the NPC's `VTCK` → the indexed `VTYP` records.
//! `.fuz` (Skyrim+) is a different container — not handled here, see
//! the design doc's Phase V notes. FNV DLC voice ships inside
//! `<DLC> - Main.bsa`, which the profile does not mount: pass it with
//! `--sounds-bsa` for a voiced DLC session.
//!
//! Playback rides the `SoundArchiveProvider` (opened through the
//! profile's sounds list — FNV's Voices BSA joins it with this phase)
//! and the `VoiceSoundCache`/`AudioWorld` pair: each response segment is
//! decoded once per LRU residency and scheduled sequentially (segment
//! *n* starts when *n-1* ends, via kira's delayed start). #5382 — voice
//! does **not** use the shared `SoundCache`: its keyspace is the
//! 105k-entry Voices archive (every distinct spoken line a new key,
//! ~0.19 MB PCM per voiced second), so it rides the byte-budgeted LRU
//! instead, and a session with no active audio device resolves nothing
//! at all (no extract, no decode — the `AudioWorld::is_active` gate
//! runs before any archive work). The total voice duration replaces the
//! subtitle presentation estimate wherever the runtime had one (the
//! Phase-L Goodbye close). A line with no resolvable voice falls back to
//! the estimate exactly as before — missing audio is the common
//! modded-game shape, not an error.

use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::misc::dialogue::InfoRecord;

use crate::asset_provider::audio::SoundArchiveProvider;

/// Quest-part length of a voice file name (#5395).
const VOICE_QUEST_CHARS: usize = 10;
/// Combined quest + topic length of a voice file name (#5395).
const VOICE_NAME_CHARS: usize = 25;

/// Candidate archive paths for one response segment, authored try-order:
/// the truncated `<quest>_<topic>` shape, then the full-EDID shape radio
/// quests use (only when it differs). Empty for a quest-less line — it
/// has no authored shape. `local_id` is the plugin-local INFO id (load
/// order byte stripped). Everything lowercased and backslash-separated —
/// the archive path convention.
pub(crate) fn voice_path_candidates(
    plugin: &str,
    voice_type: &str,
    quest_edid: &str,
    topic_edid: &str,
    local_id: u32,
    response: u8,
) -> Vec<String> {
    if quest_edid.is_empty() || topic_edid.is_empty() {
        return Vec::new();
    }
    let quest = quest_edid.to_ascii_lowercase();
    let topic = topic_edid.to_ascii_lowercase();
    let quest_cut: String = quest.chars().take(VOICE_QUEST_CHARS).collect();
    let topic_cut: String = topic
        .chars()
        .take(VOICE_NAME_CHARS.saturating_sub(quest_cut.len()))
        .collect();
    let path = |q: &str, t: &str| {
        format!(
            "sound\\voice\\{plugin}\\{voice_type}\\{q}_{t}_{:08x}_{response}.ogg",
            local_id & 0x00FF_FFFF
        )
    };
    let mut out = vec![path(&quest_cut, &topic_cut)];
    if quest_cut != quest || topic_cut != topic {
        out.push(path(&quest, &topic));
    }
    out
}

/// The owning plugin's lowercase basename and the plugin-local id of an
/// INFO FormID (#5393/#5395). Resolves through the session's
/// [`GlobalFormIdResolver`] — the same slot decode every other
/// FormID → plugin consumer uses (#3366), ESL/medium slots included —
/// then names the loaded plugin whose identity owns it. `None` when the
/// FormID belongs to no plugin in this session.
fn voice_owner(world: &World, form_id: u32) -> Option<(String, u32)> {
    // Each guard drops before the next is taken.
    let pair = world
        .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()?
        .resolve(form_id)?;
    let (masters, esm_path) = {
        let set = world.try_resource::<crate::cell_loader::LoadedPluginSet>()?;
        (set.masters.clone(), set.esm_path.clone())
    };
    masters
        .iter()
        .chain(std::iter::once(&esm_path))
        .map(|path| crate::cell_loader::load_order::plugin_basename_lc(path))
        .find(|name| byroredux_core::form_id::PluginId::from_filename(name) == pair.plugin)
        .map(|name| (name, pair.local.0))
}

/// The decode half of [`play_line_voice`]: walk the response segments
/// through the `VoiceSoundCache` (#5382 — the byte-budgeted LRU, not the
/// process-lifetime `SoundCache`). Split out so the lock-order contract
/// (#5383) is exercisable without an active audio device — the
/// `AudioWorld::is_active` gate lives in the caller.
#[allow(clippy::too_many_arguments)] // the pieces of one voice-line lookup; a struct would just re-name them
fn resolve_voice_segments(
    world: &World,
    plugin: &str,
    voice_type: &str,
    quest_edid: &str,
    topic_edid: &str,
    local_id: u32,
    info_form_id: u32,
    responses: &[u8],
) -> Option<Vec<std::sync::Arc<byroredux_audio::Sound>>> {
    let mut sounds = Vec::with_capacity(responses.len());
    {
        // #5383 — cache → provider, `combat_anim`'s order: the loader
        // reads the archive provider INSIDE `get_or_load`, under the
        // cache write guard. The pre-fix shape took the provider read
        // first and the cache write under it — the reverse edge — so the
        // type-keyed lock-order graph recorded both directions across the
        // two systems and any detector session covering a voiced line
        // and a first-time combat sound closed a cycle.
        let mut cache = world.try_resource_mut::<byroredux_audio::VoiceSoundCache>()?;
        for (segment, &response) in responses.iter().enumerate() {
            let candidates = voice_path_candidates(
                plugin,
                voice_type,
                quest_edid,
                topic_edid,
                local_id,
                response,
            );
            let sound = candidates.iter().find_map(|path| {
                cache.get_or_load(path, || {
                    let provider = world.try_resource::<SoundArchiveProvider>()?;
                    if provider.is_empty() {
                        return None;
                    }
                    provider.extract(path)
                })
            });
            let Some(sound) = sound else {
                log::debug!(
                    "#5367 V: no voice file for info {info_form_id:#X} segment {response} \
                     (voice type '{voice_type}', plugin {plugin}, quest '{quest_edid}', \
                     topic '{topic_edid}')",
                );
                if segment == 0 {
                    // The line's first segment is its voice; without it
                    // there is nothing to schedule. Later gaps keep the
                    // schedule but shorten the line (logged below).
                    return None;
                }
                break;
            };
            sounds.push(sound);
        }
    }
    if sounds.is_empty() {
        return None;
    }
    Some(sounds)
}

/// Resolve, decode, and schedule the INFO's voice segments at the NPC.
/// Returns the total voice seconds when at least one segment resolved;
/// `None` keeps the caller's subtitle estimate. `topic_edid` /
/// `quest_form_id` come from the applied selection — the file name is
/// composed from the owning quest's and topic's EDIDs, not the INFO's
/// (INFO records routinely have none).
pub(crate) fn play_line_voice(
    world: &World,
    npc: EntityId,
    info: &InfoRecord,
    topic_edid: &str,
    quest_form_id: Option<u32>,
) -> Option<f64> {
    // #5382 — a session with no audio device resolves no voice at all:
    // no archive extract, no decode, no cache fill. The pre-fix shape
    // populated the cache before `AudioWorld` discarded the play.
    {
        let audio = world.try_resource::<byroredux_audio::AudioWorld>()?;
        if !audio.is_active() {
            return None;
        }
    }
    // #5410 — a speaker with no `GlobalTransform` is mid-despawn or
    // never placed: skip playback instead of voicing from the world
    // origin (the old `unwrap_or_default`). Read up front so a
    // transform-less NPC does no archive work either.
    let position = world.get::<GlobalTransform>(npc).map(|t| t.translation)?;
    let index = world
        .try_resource::<crate::cell_loader::LoadedCellIndex>()?
        .0
        .clone();
    // NPC → base → VTCK → voice-type EDID (the archive folder).
    let Some(base) = world
        .get::<byroredux_scripting::SceneAliasCandidate>(npc)
        .map(|candidate| candidate.base_form_id)
    else {
        log::debug!("#5367 V: npc {npc} has no alias candidate");
        return None;
    };
    let Some(npc_record) = index.npcs.get(&base) else {
        log::debug!("#5367 V: base {base:#X} not in npcs index");
        return None;
    };
    let Some(voice_type) = index
        .voice_types
        .get(&npc_record.voice_form_id)
        .map(|vt| vt.editor_id.to_ascii_lowercase())
    else {
        log::debug!(
            "#5367 V: voice type {:#X} not indexed (npc base {base:#X})",
            npc_record.voice_form_id
        );
        return None;
    };
    let Some((plugin, local_id)) = voice_owner(world, info.form_id) else {
        log::debug!("#5367 V: no plugin for info {:#X}", info.form_id);
        return None;
    };
    let quest_edid = quest_form_id
        .and_then(|form_id| index.quests.get(&form_id))
        .map(|quest| quest.editor_id.as_str())
        .unwrap_or_default();

    // #5395 — each segment's file carries its authored response number;
    // a response-less INFO still voices as `_1`.
    let responses: Vec<u8> = if info.responses.is_empty() {
        vec![1]
    } else {
        info.responses
            .iter()
            .enumerate()
            .map(|(i, segment)| match segment.response_number {
                0 => (i + 1).min(u8::MAX as usize) as u8,
                n => n,
            })
            .collect()
    };
    let sounds = resolve_voice_segments(
        world,
        &plugin,
        &voice_type,
        quest_edid,
        topic_edid,
        local_id,
        info.form_id,
        &responses,
    )?;

    let mut total = 0.0;
    {
        let mut audio = world.try_resource_mut::<byroredux_audio::AudioWorld>()?;
        for sound in &sounds {
            let duration = sound.duration().as_secs_f64();
            let at = byroredux_audio::with_start_delay(sound, total);
            // #5410 — entity-anchored, not fire-and-forget: the follow
            // pass repositions each segment's track at the NPC while it
            // talks, and `stop_sounds_for` gives the conversation-close
            // path a handle to cut the line with.
            audio.play_oneshot_following(
                npc,
                at,
                position,
                byroredux_audio::Attenuation::default(),
                1.0,
            );
            total += duration;
        }
    }
    log::info!(
        "#5367 Phase V: voice line info {:#010X} -> {} segment(s), {:.1}s \
         (voice type '{}', plugin {})",
        info.form_id,
        sounds.len(),
        total,
        voice_type,
        plugin,
    );
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The authored path shape: short EDIDs need no truncation, so the
    /// truncated shape is the only candidate.
    #[test]
    fn voice_path_candidates_match_the_authored_convention() {
        let paths = voice_path_candidates(
            "falloutnv.esm",
            "maleuniquedocmitchell",
            "VCG01",
            "GREETING",
            0x0010_7222,
            1,
        );
        assert_eq!(
            paths,
            vec!["sound\\voice\\falloutnv.esm\\maleuniquedocmitchell\\vcg01_greeting_00107222_1.ogg"]
        );
        // Quest-less lines have no authored shape (the bare FormID form
        // matched zero archive entries).
        assert!(voice_path_candidates("fallout3.esm", "v", "", "someTopic", 0x1AB, 3).is_empty());
    }

    /// #5395 (LC-D3-01) — long EDIDs truncate: quest to 10 characters,
    /// topic so the two total 25; the full-EDID (radio) shape follows.
    /// Real names from `Fallout - Voices1.bsa` / `Fallout - Voices.bsa`.
    #[test]
    fn voice_path_candidates_truncate_long_edids() {
        let paths =
            voice_path_candidates("falloutnv.esm", "vt", "VDoctors", "DoctorMedical99YES", 0x1234, 1);
        assert_eq!(paths[0], "sound\\voice\\falloutnv.esm\\vt\\vdoctors_doctormedical99ye_00001234_1.ogg");
        assert_eq!(
            paths[1],
            "sound\\voice\\falloutnv.esm\\vt\\vdoctors_doctormedical99yes_00001234_1.ogg",
            "the full-EDID radio shape stays as the fallback"
        );
        let sunny = voice_path_candidates(
            "falloutnv.esm",
            "vt",
            "VCG02",
            "VCG02GSSunnySmilesTopic004",
            0x1,
            2,
        );
        assert!(sunny[0].ends_with("\\vcg02_vcg02gssunnysmilesto_00000001_2.ogg"));
        let fo3 = voice_path_candidates("fallout3.esm", "vt", "DialogueLincoln", "GOODBYE", 0x38825, 1);
        assert!(fo3[0].ends_with("\\dialogueli_goodbye_00038825_1.ogg"));
    }

    /// #5395 (FNV-D5-01) — the file name carries the plugin-local id:
    /// a DLC INFO at load-order slot 1 is written with a `00` top byte.
    #[test]
    fn voice_path_candidates_strip_the_load_order_byte() {
        let paths = voice_path_candidates("deadmoney.esm", "vt", "NVDLC01Eli", "GREETING", 0x0101_1216, 1);
        assert!(paths[0].ends_with("\\nvdlc01eli_greeting_00011216_1.ogg"));
    }

    /// #5393 (GAME-D2-02 / LC-D3-02) — with `--master FalloutNV.esm --esm
    /// DeadMoney.esm` the master owns slot 0 and the DLC slot 1. The old
    /// byte-as-position mapping sent slot 0 to the `--esm` and slot 1 to
    /// the master; the resolver names each INFO's real owner.
    #[test]
    fn voice_owner_resolves_masters_before_the_esm() {
        use crate::cell_loader::load_order::{GlobalFormIdResolver, LoadOrder};
        use byroredux_plugin::esm::reader::GlobalSlot;
        let mut world = World::new();
        world.insert_resource(crate::cell_loader::LoadedPluginSet {
            masters: vec!["/data/FalloutNV.esm".into()],
            esm_path: "/data/DeadMoney.esm".into(),
        });
        let order = LoadOrder::new(
            vec!["falloutnv.esm".into(), "deadmoney.esm".into()],
            vec![GlobalSlot::Regular(0), GlobalSlot::Regular(1)],
        );
        world.insert_resource(GlobalFormIdResolver::from_load_order(&order));
        assert_eq!(
            voice_owner(&world, 0x0010_7222),
            Some(("falloutnv.esm".to_string(), 0x0010_7222))
        );
        assert_eq!(
            voice_owner(&world, 0x0101_1216),
            Some(("deadmoney.esm".to_string(), 0x0001_1216))
        );
        assert_eq!(voice_owner(&world, 0x0500_0001), None, "no plugin owns slot 5");
    }

    /// The sound-path normalizer accepts the voice form unchanged (it
    /// already carries the `sound\` prefix and lowercase convention).
    #[test]
    fn voice_paths_are_archive_ready() {
        let path =
            voice_path_candidates("falloutnv.esm", "vtype", "q", "t", 1, 1).remove(0);
        assert_eq!(
            crate::asset_provider::audio::sound_archive_path(&path),
            path
        );
    }
    /// #5383 — one process, both sound-cache consumers: a first-time
    /// combat sound (`combat_anim::play_oneshot_cached` on the shared
    /// `SoundCache`) and a voiced dialogue line (the `VoiceSoundCache`
    /// through `resolve_voice_segments`) acquire their cache and the
    /// archive provider in ONE order (cache → provider, inside
    /// `get_or_load`). The pre-fix voice path took the provider read
    /// first and the cache write under it, so the type-keyed lock-order
    /// graph recorded both directions and any detector session
    /// (`BYRO_LOCK_ORDER_CHECK=1`) covering both closed a cycle. The
    /// decode half is driven directly because `play_line_voice`'s
    /// `AudioWorld::is_active` gate (#5382) returns before any guard on
    /// a headless world — the acquisition order lives here, unchanged.
    #[test]
    fn voice_and_combat_sound_paths_share_one_cache_provider_order() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::SoundCache::default());
        world.insert_resource(byroredux_audio::VoiceSoundCache::default());
        world.insert_resource(crate::asset_provider::audio::SoundArchiveProvider::new());
        world.insert_resource(byroredux_audio::AudioWorld::headless());

        // The combat-sound consumer: a first-time (cache-miss) path, so
        // its loader takes the provider guard under the cache write.
        crate::systems::combat_anim::play_oneshot_cached(
            &world,
            r"sound\fx\ui\pipboy\pipboy_mode.wav",
            byroredux_core::math::Vec3::ZERO,
        );

        // The dialogue-voice consumer's decode half: same
        // cache-write → in-loader provider-read shape.
        let voiced = resolve_voice_segments(
            &world,
            "falloutnv.esm",
            "maleuniquedocmitchell",
            "vcg01",
            "GREETING",
            0x0010_7222,
            0x0010_7222,
            &[1],
        );
        assert!(voiced.is_none(), "an empty archive voices nothing");
        // And the pairing does not panic under BYRO_LOCK_ORDER_CHECK=1 —
        // that is this test's role in the detector lane.
    }

    /// #5382 — a session with no active audio device resolves no voice:
    /// `play_line_voice` returns before touching the voice cache, so a
    /// headless dialogue-heavy session decodes and retains nothing. The
    /// pre-fix shape filled the cache first and let `AudioWorld` discard
    /// the play.
    #[test]
    fn inactive_audio_session_resolves_and_caches_no_voice() {
        use crate::cell_loader::load_order::{GlobalFormIdResolver, LoadOrder};
        use byroredux_plugin::esm::reader::GlobalSlot;
        use byroredux_plugin::esm::records::MinimalEsmRecord;

        let mut world = World::new();
        world.insert_resource(byroredux_audio::VoiceSoundCache::default());
        world.insert_resource(crate::asset_provider::audio::SoundArchiveProvider::new());
        world.insert_resource(byroredux_audio::AudioWorld::headless());

        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.npcs.insert(
            0x0000_0001,
            byroredux_plugin::esm::records::NpcRecord {
                form_id: 0x0000_0001,
                voice_form_id: 0x0000_0002,
                ..Default::default()
            },
        );
        index.voice_types.insert(
            0x0000_0002,
            MinimalEsmRecord {
                form_id: 0x0000_0002,
                editor_id: "MaleUniqueDocMitchell".to_owned(),
                full_name: String::new(),
            },
        );
        world.insert_resource(crate::cell_loader::LoadedCellIndex(std::sync::Arc::new(index)));
        world.insert_resource(crate::cell_loader::LoadedPluginSet {
            masters: vec!["/data/FalloutNV.esm".into()],
            esm_path: "/data/FalloutNV.esm".into(),
        });
        let order = LoadOrder::new(
            vec!["falloutnv.esm".into()],
            vec![GlobalSlot::Regular(0)],
        );
        world.insert_resource(GlobalFormIdResolver::from_load_order(&order));

        let npc = world.spawn();
        world.insert(
            npc,
            byroredux_core::ecs::components::GlobalTransform::default(),
        );
        world.insert(
            npc,
            byroredux_scripting::SceneAliasCandidate {
                reference_form_id: 0x0010_3F2B,
                base_form_id: 0x0000_0001,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        let info = byroredux_plugin::esm::records::InfoRecord {
            form_id: 0x0010_7222,
            ..Default::default()
        };

        assert!(play_line_voice(&world, npc, &info, "GREETING", Some(0x0000_0C8C)).is_none());
        let cache = world.resource::<byroredux_audio::VoiceSoundCache>();
        assert_eq!(cache.len(), 0, "no decode may be retained");
        assert_eq!(cache.negative_len(), 0, "no miss may be cached either");
    }
}

