//! #5367 Phase V — per-line dialogue voice playback.
//!
//! Bethesda's authored convention (pinned against FNV's
//! `Fallout - Voices1.bsa`, 105 517 entries):
//! `sound\voice\<plugin file>\<voice-type EDID>\<quest EDID>_<topic
//! EDID>_<INFO formid 8-hex>_<response>.ogg` — e.g. Doc Mitchell's
//! greeting (INFO 0x00107222, quest `VCG01`, topic `GREETING`) lives at
//! `sound\voice\falloutnv.esm\maleuniquedocmitchell\vcg01_greeting_00107222_1.ogg`.
//! INFO records routinely carry **no EDID of their own** (the greeting
//! has none), so the file name is composed from the owning quest's and
//! topic's EDIDs; the bare `<formid>_<response>.ogg` shape stays in the
//! try-order as the quest-less fallback. The voice-type EDID resolves
//! through the NPC's `VTCK` → the indexed `VTYP` records; the plugin
//! segment is the file name of the plugin whose load-order slot owns the
//! INFO's FormID. `.fuz` (Skyrim+) is a different container — not
//! handled here, see the design doc's Phase V notes.
//!
//! Playback rides the `SoundArchiveProvider` (opened through the
//! profile's sounds list — FNV's Voices BSA joins it with this phase)
//! and the `SoundCache`/`AudioWorld` pair: each response segment is
//! extracted once, decoded once, and scheduled sequentially (segment
//! *n* starts when *n-1* ends, via kira's delayed start). The total
//! voice duration replaces the subtitle presentation estimate wherever
//! the runtime had one (the Phase-L Goodbye close). A line with no
//! resolvable voice falls back to the estimate exactly as before —
//! missing audio is the common modded-game shape, not an error.

use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::misc::dialogue::InfoRecord;

use crate::asset_provider::audio::SoundArchiveProvider;

/// Candidate archive paths for one response segment, authored order:
/// the quest_topic-prefixed shape first, then the bare FormID shape
/// for quest-less lines. Everything lowercased and
/// backslash-separated — the archive path convention.
pub(crate) fn voice_path_candidates(
    plugin: &str,
    voice_type: &str,
    quest_edid: &str,
    topic_edid: &str,
    info_form_id: u32,
    response: usize,
) -> Vec<String> {
    let mut out = Vec::with_capacity(2);
    if !quest_edid.is_empty() && !topic_edid.is_empty() {
        out.push(format!(
            "sound\\voice\\{plugin}\\{voice_type}\\{}_{}_{:08x}_{response}.ogg",
            quest_edid.to_ascii_lowercase(),
            topic_edid.to_ascii_lowercase(),
            info_form_id
        ));
    }
    out.push(format!(
        "sound\\voice\\{plugin}\\{voice_type}\\{:08x}_{response}.ogg",
        info_form_id
    ));
    out
}

/// The plugin-file segment for a FormID's load-order slot: byte 0 is
/// the main `--esm`, byte *k* the *k*-th `--master`. `None` when the
/// slot exceeds the session's plugin set (a cross-load-order FormID).
fn plugin_file_for(world: &World, form_id: u32) -> Option<String> {
    // Copy the two strings out — the resource guard must drop before
    // the borrow outlives it (scope-based lock tracker).
    let (esm_path, masters) = match world.try_resource::<crate::cell_loader::LoadedPluginSet>() {
        Some(set) => (set.esm_path.clone(), set.masters.clone()),
        None => return None,
    };
    let byte = (form_id >> 24) as usize;
    let raw = if byte == 0 {
        esm_path.as_str()
    } else {
        masters.get(byte - 1).map(String::as_str)?
    };
    let file = std::path::Path::new(raw)
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    Some(file)
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
    let Some(plugin) = plugin_file_for(world, info.form_id) else {
        log::debug!("#5367 V: no plugin for info {:#X}", info.form_id);
        return None;
    };
    let quest_edid = quest_form_id
        .and_then(|form_id| index.quests.get(&form_id))
        .map(|quest| quest.editor_id.as_str())
        .unwrap_or_default();

    let segments = info.responses.len().max(1);
    let mut sounds = Vec::with_capacity(segments);
    {
        let provider = world.try_resource::<SoundArchiveProvider>()?;
        if provider.is_empty() {
            return None;
        }
        let mut cache = world.try_resource_mut::<byroredux_audio::SoundCache>()?;
        for response in 1..=segments {
            let candidates = voice_path_candidates(
                &plugin,
                &voice_type,
                quest_edid,
                topic_edid,
                info.form_id,
                response,
            );
            let sound = candidates.iter().find_map(|path| {
                cache.get_or_load(path, || provider.extract(path))
            });
            let Some(sound) = sound else {
                log::debug!(
                    "#5367 V: no voice file for info {:#X} segment {response} \
                     (voice type '{voice_type}', plugin {plugin}, quest '{}', \
                     topic '{topic_edid}')",
                    info.form_id,
                    quest_edid,
                );
                if response == 1 {
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

    let position = world
        .get::<GlobalTransform>(npc)
        .map(|t| t.translation)
        .unwrap_or_default();
    let mut total = 0.0;
    {
        let mut audio = world.try_resource_mut::<byroredux_audio::AudioWorld>()?;
        for sound in &sounds {
            let duration = sound.duration().as_secs_f64();
            let at = byroredux_audio::with_start_delay(sound, total);
            audio.play_oneshot(
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

    /// The authored path shapes, in try-order: the quest_topic-prefixed
    /// convention, then the bare quest-less fallback.
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
            vec![
                "sound\\voice\\falloutnv.esm\\maleuniquedocmitchell\\vcg01_greeting_00107222_1.ogg",
                "sound\\voice\\falloutnv.esm\\maleuniquedocmitchell\\00107222_1.ogg",
            ]
        );
        // Quest-less lines (or unindexed quests) author only the bare
        // shape.
        let bare =
            voice_path_candidates("fallout3.esm", "femaleadult01default", "", "someTopic", 0x0001_AB, 3);
        assert_eq!(
            bare,
            vec!["sound\\voice\\fallout3.esm\\femaleadult01default\\000001ab_3.ogg"]
        );
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
}
