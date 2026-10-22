//! #5367 Phase V — per-line dialogue voice playback.
//!
//! **Scope: FO3 / FNV and Oblivion; Skyrim `.fuz` deferred.** There are two
//! authored conventions, not one universal Bethesda rule.
//!
//! **Oblivion (#5526)** authors no NPC `VTCK` and no `VTYP` records, so there
//! is no voice type to key on. Its voices ship (`Oblivion - Voices1/2.bsa`,
//! 38,898 `.mp3`) as `sound\voice\oblivion.esm\<voice race full name,
//! lowercased>\<m|f>\<quest>_<topic>_<INFO local id 8-hex>_<n>.mp3` —
//! untruncated, `.mp3`, keyed by the speaker's race and sex: the race's
//! `VNAM` (`RaceRecord::voice_forms`, `(male, female)`) names the *voice
//! race*, whose FULL name is the folder (the folders are argonian, breton,
//! dremora, high elf, imperial, nord and redguard; every other race borrows
//! one). See [`oblivion_voice_path_candidates`].
//!
//! **FO3 / FNV** — Bethesda's authored convention (pinned against FNV's
//! `Fallout - Voices1.bsa` and FO3's `Fallout - Voices.bsa`):
//! `sound\voice\<plugin file>\<voice-type EDID>\<quest>_<topic>_<INFO
//! local id 8-hex>_<response>.ogg` — e.g. Doc Mitchell's greeting (INFO
//! 0x00107222, quest `VCG01`, topic `GREETING`) lives at
//! `sound\voice\falloutnv.esm\maleuniquedocmitchell\vcg01_greeting_00107222_1.ogg`.
//!
//! - **Truncated EDIDs (#5395, #5521).** The rule is a length test, not a
//!   per-quest exception: when `len(quest) + len(topic) > 25` the quest
//!   EDID is cut to 10 characters and the topic EDID so the two total 25
//!   (`VDoctors` + `DoctorMedical99YES` → `vdoctors_doctormedical99ye`);
//!   otherwise both keep their full EDIDs (`DialogueRivetCity` +
//!   `Greeting`). Measured against FO3's `Fallout - Voices.bsa`: all 5,442
//!   cut-shape files sit at 26..61 combined characters and all 3,592
//!   full-shape files (55 quests, none of the top twelve a radio quest) at
//!   14..25; FNV's `Voices1.bsa` partitions the same way (16,876 cut /
//!   3,961 full). Measured (LC-D3-01): the full shape alone resolved 28%
//!   of FNV's 52,876 joinable lines.
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
//! `.fuz` (Skyrim / FO4 / FO76) rides the same name shape with the
//! `.fuz` extension ([`fuz_voice_path_candidates`]); the FUZE container
//! (FUZE + raw lip + RIFF xWMA, #5464) is parsed in
//! `byroredux_audio::fuz` and its WMA payload decoded through the
//! host's optional `ffmpeg` (#5912 — parse, don't link: without
//! `ffmpeg` the line stays on the subtitle estimate exactly as
//! pre-#5912). FNV DLC voice ships inside
//! `<DLC> - Main.bsa`, which the profile does not mount: pass it with
//! `--sounds-bsa` for a voiced DLC session (the SE profile mounts
//! `Skyrim - Voices_en0.bsa` the same way).
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

/// Candidate archive paths for one response segment. The authored name is
/// decided by length (#5521): a combined quest + topic EDID over
/// [`VOICE_NAME_CHARS`] takes the truncated shape, anything else the full
/// EDIDs, so there is at most one candidate (a `Vec` stays for the
/// caller's probe loop). Empty for a quest-less line — it has no
/// authored shape. `local_id` is the plugin-local INFO id (load
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
    let (quest, topic) = if quest.chars().count() + topic.chars().count() > VOICE_NAME_CHARS {
        let quest_cut: String = quest.chars().take(VOICE_QUEST_CHARS).collect();
        let topic_cut: String = topic
            .chars()
            .take(VOICE_NAME_CHARS.saturating_sub(quest_cut.len()))
            .collect();
        (quest_cut, topic_cut)
    } else {
        (quest, topic)
    };
    vec![format!(
        "sound\\voice\\{plugin}\\{voice_type}\\{quest}_{topic}_{:08x}_{response}.ogg",
        local_id & 0x00FF_FFFF
    )]
}

/// Candidate archive paths for one Skyrim / FO4 / FO76 response segment
/// (#5912): the FO3/FNV name shape with the `.fuz` extension, and one
/// relaxation the FO3/FNV census never needed — ONE of the two EDID
/// segments may be empty (SE census: `dialoguegenericscenedog01__
/// 0005b1b6_1.fuz`, a generic line authored with a bare quest EDID and
/// no topic). The truncation rule is the same length test (#5395),
/// re-measured against the SE voice archive: `wimourning` +
/// `wimourningadult` = 25 sits exactly at the full-shape boundary,
/// while `DialogueWinterhold`'s pair ships cut to
/// `dialoguewi_dialoguewinterh` (10 + 15).
pub(crate) fn fuz_voice_path_candidates(
    plugin: &str,
    voice_type: &str,
    quest_edid: &str,
    topic_edid: &str,
    local_id: u32,
    response: u8,
) -> Vec<String> {
    if quest_edid.is_empty() && topic_edid.is_empty() {
        return Vec::new();
    }
    let quest = quest_edid.to_ascii_lowercase();
    let topic = topic_edid.to_ascii_lowercase();
    let (quest, topic) = if quest.chars().count() + topic.chars().count() > VOICE_NAME_CHARS {
        let quest_cut: String = quest.chars().take(VOICE_QUEST_CHARS).collect();
        let topic_cut: String = topic
            .chars()
            .take(VOICE_NAME_CHARS.saturating_sub(quest_cut.len()))
            .collect();
        (quest_cut, topic_cut)
    } else {
        (quest, topic)
    };
    vec![format!(
        "sound\\voice\\{plugin}\\{voice_type}\\{quest}_{topic}_{:08x}_{response}.fuz",
        local_id & 0x00FF_FFFF
    )]
}

/// Candidate archive paths for one Oblivion response segment (#5526):
/// `sound\voice\<plugin>\<voice race>\<m|f>\<quest>_<topic>_<local id
/// 8-hex>_<response>.mp3`. Unlike the FO3/FNV shape the EDIDs are never
/// truncated (quests run to 22 characters, quest + topic to 46 in
/// `Oblivion - Voices1/2.bsa`). `voice_race` is the voice race's lowercased
/// FULL name; `female` picks the `f` / `m` folder. Empty for a quest-less
/// line, which has no authored shape.
pub(crate) fn oblivion_voice_path_candidates(
    plugin: &str,
    voice_race: &str,
    female: bool,
    quest_edid: &str,
    topic_edid: &str,
    local_id: u32,
    response: u8,
) -> Vec<String> {
    if voice_race.is_empty() || quest_edid.is_empty() || topic_edid.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "sound\\voice\\{plugin}\\{voice_race}\\{}\\{}_{}_{:08x}_{response}.mp3",
        if female { 'f' } else { 'm' },
        quest_edid.to_ascii_lowercase(),
        topic_edid.to_ascii_lowercase(),
        local_id & 0x00FF_FFFF
    )]
}

/// The lowercased FULL name of the voice race an Oblivion NPC speaks with:
/// its RACE's `VNAM` `(male, female)` voice race for the NPC's sex, resolved
/// to that RACE's name. `None` when the race, its `VNAM` or the voice race's
/// name is missing — the line then keeps the subtitle estimate rather than
/// guessing a folder (#5526).
pub(crate) fn oblivion_voice_race(
    index: &byroredux_plugin::esm::records::EsmIndex,
    npc: &byroredux_plugin::esm::records::NpcRecord,
) -> Option<String> {
    use byroredux_plugin::equip::Gender;
    let (male, female) = index.races.get(&npc.race_form_id)?.voice_forms?;
    let voice_race = match Gender::from_acbs_flags(npc.acbs_flags) {
        Gender::Female => female,
        _ => male,
    };
    let name = index.races.get(&voice_race)?.full_name.to_ascii_lowercase();
    (!name.is_empty()).then_some(name)
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
    // The speaker's voice key for the log line only: the voice-type EDID
    // (FO3/FNV) or the voice race (Oblivion).
    voice_label: &str,
    quest_edid: &str,
    topic_edid: &str,
    info_form_id: u32,
    responses: &[u8],
    // The per-game archive-path convention for one response number.
    candidates_for: &dyn Fn(u8) -> Vec<String>,
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
            let candidates = candidates_for(response);
            let sound = candidates.iter().find_map(|path| {
                cache.get_or_load(path, || {
                    let provider = world.try_resource::<SoundArchiveProvider>()?;
                    if provider.is_empty() {
                        return None;
                    }
                    let bytes = provider.extract(path)?;
                    // #5912 — FUZE containers (Skyrim/FO4/FO76 .fuz) must
                    // become WAV before the cache's kira decode sees them;
                    // every other vanilla voice payload is already
                    // kira-decodable. `ffmpeg` missing is the expected
                    // degradation (debug, once per line) — a parse failure
                    // on a FUZE-marked file is worth a warn.
                    if bytes.starts_with(b"FUZE") {
                        match byroredux_audio::fuz::fuz_to_wav_bytes(&bytes) {
                            Ok(wav) => Some(wav),
                            Err(byroredux_audio::fuz::FuzDecodeError::FfmpegMissing) => {
                                log::debug!(
                                    "#5912: '{path}' resolved but no host ffmpeg — \
                                     playing silent on the subtitle estimate"
                                );
                                None
                            }
                            Err(e) => {
                                log::warn!("#5912: .fuz decode failed for '{path}': {e}");
                                None
                            }
                        }
                    } else {
                        Some(bytes)
                    }
                })
            });
            let Some(sound) = sound else {
                log::debug!(
                    "#5367 V: no voice file for info {info_form_id:#X} segment {response} \
                     (voice '{voice_label}', plugin {plugin}, quest '{quest_edid}', \
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

/// M47.3 — play the lines ObScript `Say` / `SayTo` made actors say. Each is voiced like a
/// conversation line (the topic's and owning quest's EDIDs name the file); a recording that
/// resolves replaces the subtitle estimate the call answered with as the line's length, which
/// is what times its `SayToDone`. A line with no recording stays silent and keeps the estimate.
pub(crate) fn scripted_line_voice_system(world: &World, _dt: f32) {
    let lines = match world.try_resource_mut::<byroredux_scripting::obscript_speech::PendingScriptedLines>() {
        Some(mut lines) if !lines.pending.is_empty() => std::mem::take(&mut lines.pending),
        _ => return,
    };
    for line in lines {
        // A new line cuts the speaker's previous one.
        stop_line_voice(world, line.speaker);
        let Some(seconds) = play_line_voice(world, line.speaker, &line.info, &line.topic_editor_id, line.owning_quest) else {
            continue;
        };
        if let Some(mut lines) = world.try_resource_mut::<byroredux_scripting::obscript_speech::PendingScriptedLines>() {
            lines.set_duration(line.speaker, line.topic_form_id, seconds as f32);
        }
    }
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
    let Some(shell) = index.npcs.get(&base) else {
        log::debug!("#5367 V: base {base:#X} not in npcs index");
        return None;
    };
    // #6228 — voice type, sex and race are Use-Traits fields: a templated shell speaks with its
    // terminal's, the record its body was spawned from.
    let npc_record = byroredux_plugin::equip::traits_terminal(shell, &index);
    // The speaker's voice key. FO3/FNV: the voice-type EDID through VTCK ->
    // VTYP. Oblivion authors neither record (#5526): its key is the voice
    // race folder plus the sex folder, through the speaker's RACE `VNAM`.
    let oblivion = index.game == byroredux_plugin::esm::reader::GameKind::Oblivion;
    // #5912 — the .fuz games share the FO3/FNV name shape (extension and
    // the one-empty-segment relaxation aside). Starfield ships Wwise
    // .wem voice, not FUZE, and stays on the .ogg-shaped path that
    // resolves nothing for it.
    let fuz_game = matches!(
        index.game,
        byroredux_plugin::esm::reader::GameKind::Skyrim
            | byroredux_plugin::esm::reader::GameKind::Fallout4
            | byroredux_plugin::esm::reader::GameKind::Fallout76
    );
    let voice_key = if oblivion {
        oblivion_voice_race(&index, npc_record)
    } else {
        index
            .voice_types
            .get(&npc_record.voice_form_id)
            .map(|vt| vt.editor_id.to_ascii_lowercase())
    };
    let Some(voice_type) = voice_key else {
        log::debug!(
            "#5367 V: no voice key (voice type {:#X} / race {:#X}) for npc base {base:#X}",
            npc_record.voice_form_id,
            npc_record.race_form_id,
        );
        return None;
    };
    let female = byroredux_plugin::equip::Gender::from_acbs_flags(npc_record.acbs_flags)
        == byroredux_plugin::equip::Gender::Female;
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
    let candidates_for = |response: u8| {
        if oblivion {
            oblivion_voice_path_candidates(
                &plugin, &voice_type, female, quest_edid, topic_edid, local_id, response,
            )
        } else if fuz_game {
            fuz_voice_path_candidates(
                &plugin, &voice_type, quest_edid, topic_edid, local_id, response,
            )
        } else {
            voice_path_candidates(&plugin, &voice_type, quest_edid, topic_edid, local_id, response)
        }
    };
    let sounds = resolve_voice_segments(
        world,
        &plugin,
        &voice_type,
        quest_edid,
        topic_edid,
        info.form_id,
        &responses,
        &candidates_for,
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

/// Fade applied to a cut line: short enough to read as "the line
/// stopped", long enough that the cut does not click.
const LINE_STOP_FADE_MS: f32 = 150.0;

/// #5432 — cut `npc`'s voice line: fade the segments already playing and
/// drop the not-yet-dispatched ones. [`play_line_voice`] schedules every
/// segment entity-anchored (#5410), so the line's lifetime is owned by
/// the speaker entity and no per-line handle needs keeping on the
/// dialogue state. Called where a line stops being current — a
/// replacement selection (before the new line's segments are queued, or
/// the stop would cut them too) and the conversation closing. Returns how
/// many sounds were stopped or dequeued; 0 without an `AudioWorld`.
pub(crate) fn stop_line_voice(world: &World, npc: EntityId) -> usize {
    world
        .try_resource_mut::<byroredux_audio::AudioWorld>()
        .map_or(0, |mut audio| audio.stop_sounds_for(npc, LINE_STOP_FADE_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #5912 — the .fuz name shape, pinned against the SE voice archive
    /// census (`fuz_census`): full EDIDs at the 25-char boundary stay
    /// full (`wimourning_wimourningadult`, 10+15), longer pairs cut the
    /// same way as FO3/FNV (`DialogueWinterhold`'s pair ships as
    /// `dialoguewi_dialoguewinterh`), and the bare-quest generic lines
    /// the FO3/FNV census never saw compose with an empty topic (the
    /// double underscore is authored data, not a bug).
    #[test]
    fn fuz_voice_path_candidates_match_the_se_census() {
        let paths = fuz_voice_path_candidates(
            "skyrim.esm",
            "maleguard",
            "DA02",
            "DA02Steward",
            0x000B_3E33,
            1,
        );
        assert_eq!(
            paths,
            vec!["sound\\voice\\skyrim.esm\\maleguard\\da02_da02steward_000b3e33_1.fuz"]
        );
        // The boundary: 25 combined chars is the FULL shape (the rule
        // cuts only over 25).
        let paths = fuz_voice_path_candidates(
            "skyrim.esm",
            "femalenord",
            "WIMourning",
            "WIMourningAdult",
            0x0002_9733,
            1,
        );
        assert_eq!(
            paths,
            vec!["sound\\voice\\skyrim.esm\\femalenord\\wimourning_wimourningadult_00029733_1.fuz"]
        );
        // Over 25: quest cut to 10, topic to the remainder.
        let paths = fuz_voice_path_candidates(
            "skyrim.esm",
            "maleuniqueancano",
            "DialogueWinterhold",
            "DialogueWinterholdCivility",
            0x0008_7993,
            1,
        );
        assert_eq!(
            paths,
            vec!["sound\\voice\\skyrim.esm\\maleuniqueancano\\dialoguewi_dialoguewinterh_00087993_1.fuz"]
        );
        // A bare-quest generic line: one empty segment composes (the
        // authored file really carries the double underscore).
        let paths = fuz_voice_path_candidates(
            "skyrim.esm",
            "malecoward",
            "DialogueGenericSceneDog01",
            "",
            0x0005_B1B6,
            1,
        );
        assert_eq!(
            paths,
            vec!["sound\\voice\\skyrim.esm\\malecoward\\dialoguegenericscenedog01__0005b1b6_1.fuz"]
        );
        // A line with neither EDID has no authored shape.
        assert!(fuz_voice_path_candidates("skyrim.esm", "v", "", "", 0x1AB, 1).is_empty());
    }

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

    /// #5395 (LC-D3-01) / #5521 — the cut is a length test: EDIDs truncate
    /// (quest to 10, topic so the two total 25) only when their combined
    /// length exceeds 25; at or under 25 both keep their full EDIDs. A
    /// single candidate either way. Real names from `Fallout - Voices1.bsa`
    /// / `Fallout - Voices.bsa`.
    #[test]
    fn voice_path_candidates_truncate_only_over_the_length_limit() {
        let paths =
            voice_path_candidates("falloutnv.esm", "vt", "VDoctors", "DoctorMedical99YES", 0x1234, 1);
        assert_eq!(
            paths,
            vec!["sound\\voice\\falloutnv.esm\\vt\\vdoctors_doctormedical99ye_00001234_1.ogg"],
            "26 combined characters: cut shape only, no full-EDID probe"
        );
        let sunny = voice_path_candidates(
            "falloutnv.esm",
            "vt",
            "VCG02",
            "VCG02GSSunnySmilesTopic004",
            0x1,
            2,
        );
        assert_eq!(
            sunny,
            vec!["sound\\voice\\falloutnv.esm\\vt\\vcg02_vcg02gssunnysmilesto_00000001_2.ogg"]
        );
        // FO3 `DialogueRivetCity` + `Greeting` = 25: authored in full, and a
        // quest longer than 10 characters is NOT cut when the total fits.
        let rivet = voice_path_candidates("fallout3.esm", "vt", "DialogueRivetCity", "Greeting", 0x38825, 1);
        assert_eq!(
            rivet,
            vec!["sound\\voice\\fallout3.esm\\vt\\dialoguerivetcity_greeting_00038825_1.ogg"]
        );
        // One character over the limit flips to the cut shape.
        let over = voice_path_candidates("fallout3.esm", "vt", "DialogueRivetCity", "Greetings", 0x38825, 1);
        assert_eq!(
            over,
            vec!["sound\\voice\\fallout3.esm\\vt\\dialogueri_greetings_00038825_1.ogg"]
        );
        // Radio quests carry no special case: `RadioNewVegas` + `Hello` fits.
        let radio = voice_path_candidates("falloutnv.esm", "vt", "RadioNewVegas", "Hello", 0x2, 1);
        assert!(radio[0].ends_with("\\radionewvegas_hello_00000002_1.ogg"));
    }

    /// #5395 (FNV-D5-01) — the file name carries the plugin-local id:
    /// a DLC INFO at load-order slot 1 is written with a `00` top byte.
    /// #5526 — Oblivion's authored shape, pinned to a file that exists in
    /// `Oblivion - Voices1.bsa`: `sound\voice\oblivion.esm\high elf\f\
    /// dark17following_greeting_0000566f_1.mp3`. The EDIDs are never
    /// truncated (quests run to 22 characters, quest + topic to 46), the
    /// extension is `.mp3`, the folders are voice race then sex.
    #[test]
    fn oblivion_voice_paths_match_the_authored_convention() {
        let paths = oblivion_voice_path_candidates(
            "oblivion.esm",
            "high elf",
            true,
            "Dark17Following",
            "GREETING",
            0x0000_566F,
            1,
        );
        assert_eq!(
            paths,
            vec!["sound\\voice\\oblivion.esm\\high elf\\f\\dark17following_greeting_0000566f_1.mp3"]
        );
        // Male folder, the load-order byte stripped, and no truncation however
        // long the pair is (the FO3/FNV rule would cut this at 25 characters).
        let long = oblivion_voice_path_candidates(
            "oblivion.esm",
            "breton",
            false,
            "AVeryLongQuestEdid22ch",
            "AnEquallyLongTopicEdidForTheTest",
            0x0100_00AB,
            3,
        );
        assert_eq!(
            long,
            vec!["sound\\voice\\oblivion.esm\\breton\\m\\averylongquestedid22ch_anequallylongtopicedidforthetest_000000ab_3.mp3"]
        );
        // A quest-less line, or a speaker with no voice race, has no authored shape.
        assert!(oblivion_voice_path_candidates("oblivion.esm", "nord", false, "", "T", 1, 1).is_empty());
        assert!(oblivion_voice_path_candidates("oblivion.esm", "", false, "Q", "T", 1, 1).is_empty());
    }

    /// #5526 — the voice race comes from the speaker's RACE `VNAM`
    /// `(male, female)` by sex, resolved to that RACE's FULL name; a race
    /// borrowing another's voice (Dark Elf speaking with the Breton folder)
    /// is the point. Anything missing is `None`, never a guessed folder.
    #[test]
    fn oblivion_voice_race_follows_the_speakers_race_vnam_by_sex() {
        use byroredux_plugin::esm::records::{EsmIndex, NpcRecord, RaceRecord};
        let race = |form_id: u32, name: &str, voice_forms: Option<(u32, u32)>| RaceRecord {
            form_id,
            full_name: name.to_string(),
            voice_forms,
            ..Default::default()
        };
        let mut index = EsmIndex::default();
        index.races.insert(0x10, race(0x10, "Dark Elf", Some((0x11, 0x12))));
        index.races.insert(0x11, race(0x11, "High Elf", None));
        index.races.insert(0x12, race(0x12, "Breton", None));
        index.races.insert(0x13, race(0x13, "Nameless", Some((0x14, 0x14))));
        index.races.insert(0x14, race(0x14, "", None));
        index.races.insert(0x15, race(0x15, "No Vnam", None));
        let npc = |race_form_id: u32, acbs_flags: u32| NpcRecord {
            race_form_id,
            acbs_flags,
            ..Default::default()
        };

        assert_eq!(oblivion_voice_race(&index, &npc(0x10, 0)).as_deref(), Some("high elf"));
        assert_eq!(oblivion_voice_race(&index, &npc(0x10, 1)).as_deref(), Some("breton"));
        assert_eq!(oblivion_voice_race(&index, &npc(0x13, 0)), None, "an unnamed voice race");
        assert_eq!(oblivion_voice_race(&index, &npc(0x15, 0)), None, "a race with no VNAM");
        assert_eq!(oblivion_voice_race(&index, &npc(0x99, 0)), None, "an unknown race");
    }

    /// #5526 against the installed archive: the example path from the audit
    /// resolves in `Oblivion - Voices1.bsa` and decodes as `.mp3` through the
    /// same provider and decoder the playback path uses.
    #[test]
    #[ignore = "needs Oblivion game data on disk"]
    fn an_installed_oblivion_voice_line_resolves_and_decodes() {
        let dir = std::env::var_os("BYROREDUX_OBLIVION_DATA")
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data".into()
            });
        let archive = dir.join("Oblivion - Voices1.bsa");
        if !archive.is_file() {
            eprintln!("skipping: {archive:?} not installed");
            return;
        }
        let args = vec!["--sounds-bsa".to_string(), archive.to_string_lossy().into_owned()];
        let provider = crate::asset_provider::audio::build_sound_archive_provider(&args);
        let path = oblivion_voice_path_candidates(
            "oblivion.esm",
            "high elf",
            true,
            "Dark17Following",
            "Greeting",
            0x0000_566F,
            3,
        )
        .remove(0);
        let bytes = provider.extract(&path).unwrap_or_else(|| panic!("{path} is not in the archive"));
        let sound = byroredux_audio::load_sound_from_bytes(bytes).expect("the .mp3 decodes");
        assert!(sound.duration().as_secs_f64() > 0.0);
    }

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
            &[1],
            &|response| {
                voice_path_candidates(
                    "falloutnv.esm",
                    "maleuniquedocmitchell",
                    "vcg01",
                    "GREETING",
                    0x0010_7222,
                    response,
                )
            },
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

    /// #5432 — `stop_line_voice` is safe on every audio configuration a
    /// session can have: no `AudioWorld` at all, and a headless one
    /// (nothing is ever queued or playing, so nothing is stopped).
    #[test]
    fn stop_line_voice_is_a_noop_without_a_live_audio_device() {
        let mut world = World::new();
        let npc = world.spawn();
        assert_eq!(stop_line_voice(&world, npc), 0, "no AudioWorld resource");
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        assert_eq!(stop_line_voice(&world, npc), 0, "headless AudioWorld");
    }
}
