//! The game-neutral music controller (`docs/engine/music.md` §2): one directive per layer, what is
//! audible, which track plays, the fades between them, and the report.

use super::directive::{Content, Directive, Outcome, Shuffle};
use std::collections::HashSet;

/// The layers below Hold, top to bottom (PRD §3.1). Hold is a flag, not a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layer {
    Sting = 0,
    Script = 1,
    Battle = 2,
    Location = 3,
}

impl Layer {
    pub(crate) const ALL: [Layer; 4] = [Layer::Sting, Layer::Script, Layer::Battle, Layer::Location];

    fn name(self) -> &'static str {
        match self {
            Layer::Sting => "Sting",
            Layer::Script => "Script",
            Layer::Battle => "Battle",
            Layer::Location => "Location",
        }
    }

    /// The music voices a layer owns: `layer * 8 + slot`; slot 0 is a playlist's track, slots 0
    /// to 5 a media set's stems, slot 7 a battle intro or outro.
    fn voice(self, slot: u32) -> u32 {
        self as u32 * 8 + slot
    }
}

const ONE_SHOT_SLOT: u32 = 7;

/// Where the controller's decisions are heard: the audio world's music voices, or nowhere (no audio
/// device, a test). Keys are the sound provider's `music\` keys.
pub(crate) trait MusicOutput {
    /// Whether tracks really play: only then does a track end (a headless run reports the first
    /// track of each directive, `docs/engine/music.md` §8).
    fn live(&self) -> bool;
    /// Start `key` on `voice`; `false` when it could not be read or decoded.
    fn play(&mut self, voice: u32, key: &str, level: f32, fade: f32, looping: bool) -> bool;
    fn stop(&mut self, voice: u32, fade: f32);
    fn pause(&mut self, voice: u32, fade: f32);
    fn resume(&mut self, voice: u32, fade: f32);
    /// Whether `voice` still holds an unfinished track.
    fn active(&self, voice: u32) -> bool;
    fn position(&self, voice: u32) -> Option<f64>;
}

/// A track a layer is playing.
#[derive(Debug, Clone)]
struct Playing {
    key: String,
    voice: u32,
    /// Seconds played on the controller's own clock, which a suspended layer stops.
    clock: f64,
}

#[derive(Debug, Default)]
struct LayerState {
    directive: Option<Directive>,
    /// A playlist's current track.
    track: Option<Playing>,
    /// A media set's stems, by stem index, while each plays.
    stems: Vec<Option<Playing>>,
    /// A battle intro or outro playing on the one-shot slot.
    one_shot: Option<Playing>,
    shuffle: Shuffle,
    /// The layer (or Hold) that suspends this one, while one does.
    suspended: Option<&'static str>,
}

impl LayerState {
    fn voices(&self) -> impl Iterator<Item = &Playing> {
        self.track.iter().chain(self.stems.iter().flatten()).chain(self.one_shot.iter())
    }

    fn voices_mut(&mut self) -> impl Iterator<Item = &mut Playing> {
        self.track.iter_mut().chain(self.stems.iter_mut().flatten()).chain(self.one_shot.iter_mut())
    }

    fn stop_all(&mut self, out: &mut dyn MusicOutput, fade: f32) {
        for playing in self.voices() {
            out.stop(playing.voice, fade);
        }
        self.track = None;
        self.stems.clear();
        self.one_shot = None;
    }

}

/// What the world tells the controller each frame.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Inputs {
    pub(crate) dt: f32,
    /// A movie's pause-music flag (MUS-MIX-002).
    pub(crate) hold: bool,
    pub(crate) in_combat: bool,
    /// The player's position, Z-up, for a media set's area.
    pub(crate) player: Option<[f32; 3]>,
    /// The hour of the day (0..24), for a media set's day and night.
    pub(crate) hour: f32,
}

/// The music state the engine keeps. `NOT_SAVED_BY_DESIGN` (Q5, MUS-SAV-002): every layer
/// re-resolves after a load.
pub(crate) struct MusicController {
    layers: [LayerState; 4],
    hold: bool,
    /// Seconds Battle still holds after combat ended (MUS-CMB-002).
    battle_hold: f32,
    /// Sources and paths already logged as Unresolved or Missing this session (MUS-RPT-002).
    reported: HashSet<String>,
}

impl Default for MusicController {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9, |d| d.as_nanos() as u64);
        Self::seeded(seed)
    }
}

impl byroredux_core::ecs::Resource for MusicController {}

impl MusicController {
    pub(crate) fn seeded(seed: u64) -> Self {
        let mut layers: [LayerState; 4] = Default::default();
        for (i, layer) in layers.iter_mut().enumerate() {
            layer.shuffle = Shuffle::seeded(seed.wrapping_add((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        }
        Self { layers, hold: false, battle_hold: 0.0, reported: HashSet::new() }
    }

    fn state(&self, layer: Layer) -> &LayerState {
        &self.layers[layer as usize]
    }

    #[cfg(test)]
    pub(crate) fn directive(&self, layer: Layer) -> Option<&Directive> {
        self.state(layer).directive.as_ref()
    }

    /// Report a directive that is not played (a missing `StreamMusic` file): once per session.
    pub(crate) fn note(&mut self, layer: Layer, directive: &Directive) {
        self.report(layer, directive);
    }

    /// Log an Unresolved source or a Missing path once per session (MUS-RPT-002).
    fn report(&mut self, layer: Layer, directive: &Directive) {
        let (what, detail) = match &directive.content {
            Content::Unresolved(reason) => ("unresolved", reason.as_str()),
            Content::Missing(path) => ("missing", path.as_str()),
            _ => return,
        };
        if self.reported.insert(format!("{what}:{}:{detail}", directive.source)) {
            log::info!("music: {} {what}: {} — {detail}", layer.name(), directive.source);
        }
        for note in &directive.notes {
            if self.reported.insert(format!("note:{}:{note}", directive.source)) {
                log::info!("music: {} {}: {note}", layer.name(), directive.source);
            }
        }
    }

    /// Give `layer` a new directive, or none. The same music keeps playing where it is
    /// (MUS-SEL-005); anything else fades the old tracks out over the new directive's fade.
    pub(crate) fn set(&mut self, layer: Layer, directive: Option<Directive>, out: &mut dyn MusicOutput) {
        let state = &mut self.layers[layer as usize];
        match (&state.directive, &directive) {
            (Some(old), Some(new)) if old.same_music(new) => {
                state.directive = directive;
                return;
            }
            (None, None) => return,
            _ => {}
        }
        let fade = directive
            .as_ref()
            .or(state.directive.as_ref())
            .map_or(0.0, |d| d.fade_secs);
        state.stop_all(out, fade);
        state.directive = directive;
        if let Some(d) = state.directive.clone() {
            self.report(layer, &d);
        }
    }

    /// End the Script layer: a cell change or a load (MUS-SCR, MUS-SAV-002).
    pub(crate) fn end_script(&mut self, out: &mut dyn MusicOutput) {
        self.set(Layer::Script, None, out);
    }

    /// Play a sting over Battle and Location (MUS-STG-004).
    pub(crate) fn sting(&mut self, directive: Directive, out: &mut dyn MusicOutput) {
        // A second sting restarts the slot rather than being merged into the first.
        self.set(Layer::Sting, None, out);
        self.set(Layer::Sting, Some(directive), out);
    }

    /// Advance one frame: Battle's start and hold, what each layer may play, and which track.
    /// `battle` is asked for the Battle directive only when combat starts.
    pub(crate) fn step(&mut self, inputs: Inputs, battle: impl FnOnce() -> Directive, out: &mut dyn MusicOutput) {
        self.hold = inputs.hold;
        self.step_battle(inputs, battle, out);

        let playing = |music: &Self, layer: Layer| music.state(layer).directive.as_ref().is_some_and(|d| d.outcome() == Outcome::Playing);
        // Top down, so a layer sees the higher layers as this frame left them (a sting that just
        // ended releases Location the same frame).
        for layer in Layer::ALL {
            let (sting_audible, battle_audible) = (playing(self, Layer::Sting), playing(self, Layer::Battle));
            // A scripted track replaces the music below it, and a scripted silence (`PlayMusic
            // 1NoMusic`) silences it (MUS-SCR-005, #6899) — unless it gives way to Battle.
            let script = self.state(Layer::Script).directive.as_ref();
            let script_gives_way = script.is_some_and(|d| d.gives_way_to_battle);
            let script_holds = script.is_some_and(|d| matches!(d.outcome(), Outcome::Playing | Outcome::Silent))
                && !(script_gives_way && battle_audible);
            let by = match layer {
                _ if inputs.hold => Some("Hold"),
                Layer::Script | Layer::Battle | Layer::Location if sting_audible => Some("Sting"),
                Layer::Script if script_gives_way && battle_audible => Some("Battle"),
                Layer::Battle | Layer::Location if script_holds => Some("Script"),
                Layer::Location if battle_audible => Some("Battle"),
                _ => None,
            };
            self.suspend(layer, by, out);
            self.play(layer, inputs, out);
        }
    }

    fn step_battle(&mut self, inputs: Inputs, battle: impl FnOnce() -> Directive, out: &mut dyn MusicOutput) {
        if inputs.in_combat {
            if self.state(Layer::Battle).directive.is_none() {
                let directive = battle();
                self.set(Layer::Battle, Some(directive), out);
                self.one_shot(Layer::Battle, true, out);
            }
            self.battle_hold = self.state(Layer::Battle).directive.as_ref().and_then(|d| d.recovery_secs).unwrap_or(0.0);
        } else if self.state(Layer::Battle).directive.is_some() {
            self.battle_hold -= inputs.dt;
            if self.battle_hold <= 0.0 {
                // A battle set's outro plays as Battle hands back (MUS-CMB-003).
                let outro = self.state(Layer::Battle).directive.as_ref().and_then(|d| d.outro.clone().map(|key| (key, d.level)));
                self.set(Layer::Battle, None, out);
                if let Some((key, level)) = outro {
                    let voice = Layer::Battle.voice(ONE_SHOT_SLOT);
                    if out.play(voice, &key, level, 0.0, false) {
                        self.layers[Layer::Battle as usize].one_shot = Some(Playing { key, voice, clock: 0.0 });
                    }
                }
            }
        }
    }

    /// A battle set's intro on entering combat (MUS-CMB-003).
    fn one_shot(&mut self, layer: Layer, intro: bool, out: &mut dyn MusicOutput) {
        let state = &mut self.layers[layer as usize];
        let Some(d) = &state.directive else { return };
        let Some(key) = (if intro { d.intro.clone() } else { d.outro.clone() }) else { return };
        let voice = layer.voice(ONE_SHOT_SLOT);
        if out.play(voice, &key, d.level, 0.0, false) {
            state.one_shot = Some(Playing { key, voice, clock: 0.0 });
        }
    }

    fn suspend(&mut self, layer: Layer, by: Option<&'static str>, out: &mut dyn MusicOutput) {
        let state = &mut self.layers[layer as usize];
        let suspended = by.is_some();
        let previous = state.suspended;
        state.suspended = by;
        if previous.is_some() == suspended {
            return;
        }
        // A movie cuts in and out: Hold pauses and releases at once, so the music stops where the
        // movie began (MUS-MIX-002). A layer above crossfades with it.
        let fade = if by == Some("Hold") || previous == Some("Hold") {
            0.0
        } else {
            state.directive.as_ref().map_or(0.0, |d| d.fade_secs)
        };
        for playing in state.voices() {
            if suspended {
                out.pause(playing.voice, fade);
            } else {
                out.resume(playing.voice, fade);
            }
        }
    }

    fn play(&mut self, layer: Layer, inputs: Inputs, out: &mut dyn MusicOutput) {
        let live = out.live();
        let state = &mut self.layers[layer as usize];
        if state.suspended.is_some() {
            return;
        }
        for playing in state.voices_mut() {
            playing.clock += f64::from(inputs.dt);
        }
        // An intro or outro plays once; with no audio device it is over at once (`docs/engine/music.md` §8).
        if state.one_shot.as_ref().is_some_and(|p| !live || !out.active(p.voice)) {
            state.one_shot = None;
        }
        let Some(d) = state.directive.clone() else { return };
        match &d.content {
            // A battle set's loop waits for its intro (MUS-CMB-003).
            Content::Playlist(_) if state.one_shot.is_some() => {}
            Content::Playlist(tracks) => {
                let ended = state.track.as_ref().is_some_and(|p| live && !d.looping && !out.active(p.voice));
                if ended {
                    state.track = None;
                    // A sting plays once; a scripted track hands back to Location (MUS-SCR-003).
                    if matches!(layer, Layer::Sting | Layer::Script) {
                        state.directive = None;
                        return;
                    }
                }
                if state.track.is_none() {
                    let index = state.shuffle.next(tracks.len());
                    let key = tracks[index].clone();
                    let voice = layer.voice(0);
                    // A one-track playlist loops its track (Q1); so does a looping directive.
                    let looping = d.looping || tracks.len() == 1 && !matches!(layer, Layer::Sting | Layer::Script);
                    if out.play(voice, &key, d.level, d.fade_secs, looping) {
                        state.track = Some(Playing { key, voice, clock: 0.0 });
                    } else {
                        let missing = Directive { content: Content::Missing(key), ..d };
                        state.directive = Some(missing.clone());
                        self.report(layer, &missing);
                    }
                }
            }
            Content::Stems { stems, area } => {
                let (depth, day) = match inputs.player {
                    Some(p) => {
                        let offset = [p[0] - area.center[0], p[1] - area.center[1], p[2] - area.center[2]];
                        let distance = (offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2]).sqrt();
                        let aloc = byroredux_plugin::esm::records::MediaLocationRecord {
                            day_start: area.day_start,
                            night_start: area.night_start,
                            ..Default::default()
                        };
                        (distance / area.radius.max(1.0), aloc.is_day(inputs.hour))
                    }
                    None => (0.0, true),
                };
                state.stems.resize(stems.len(), None);
                for (i, stem) in stems.iter().enumerate() {
                    let wanted = stem.night != day && depth <= stem.reach;
                    let voice = layer.voice(i as u32);
                    match (&state.stems[i], wanted) {
                        (None, true) => {
                            if out.play(voice, &stem.key, d.level * stem.level, d.fade_secs, true) {
                                state.stems[i] = Some(Playing { key: stem.key.clone(), voice, clock: 0.0 });
                            }
                        }
                        (Some(_), false) => {
                            out.stop(voice, d.fade_secs);
                            state.stems[i] = None;
                        }
                        _ => {}
                    }
                }
            }
            Content::Silent | Content::Unresolved(_) | Content::Missing(_) => {}
        }
    }

    /// `music.status` (MUS-RPT-001): the Hold flag, then each layer's outcome, source, file and position.
    pub(crate) fn status(&self, out: &dyn MusicOutput) -> Vec<String> {
        let mut lines = vec![format!("Hold: {}", if self.hold { "on (a movie pauses the music)" } else { "off" })];
        for layer in Layer::ALL {
            let state = self.state(layer);
            let Some(d) = &state.directive else {
                lines.push(format!("{}: none", layer.name()));
                continue;
            };
            let outcome = match (&d.content, state.suspended) {
                (Content::Playlist(_) | Content::Stems { .. }, Some(by)) => format!("Playing (paused under {by})"),
                (Content::Playlist(_) | Content::Stems { .. }, None) => "Playing".to_string(),
                (Content::Silent, _) => "Silent".to_string(),
                (Content::Unresolved(reason), _) => format!("Unresolved ({reason})"),
                (Content::Missing(path), _) => format!("Missing ({path})"),
            };
            let mut line = format!("{}: {outcome} — {}", layer.name(), d.source);
            for playing in state.voices() {
                let position = out.position(playing.voice).unwrap_or(playing.clock);
                line.push_str(&format!(" — {} at {position:.1}s", playing.key));
            }
            if layer == Layer::Battle && d.outcome() == Outcome::Playing && self.battle_hold > 0.0 {
                line.push_str(&format!(" — recovery {:.1}s", self.battle_hold));
            }
            for note in &d.notes {
                line.push_str(&format!(" — {note}"));
            }
            lines.push(line);
        }
        lines
    }

    /// The position of `layer`'s playlist track, for tests.
    #[cfg(test)]
    pub(crate) fn track(&self, layer: Layer, out: &dyn MusicOutput) -> Option<(String, f64)> {
        let playing = self.state(layer).track.as_ref()?;
        Some((playing.key.clone(), out.position(playing.voice).unwrap_or(playing.clock)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::music::directive::Source;
    use std::collections::HashMap;

    /// A recording output: every voice "plays" until a test ends it.
    #[derive(Default)]
    pub(crate) struct FakeOutput {
        pub(crate) live: bool,
        pub(crate) voices: HashMap<u32, (String, bool)>,
        pub(crate) log: Vec<String>,
    }

    impl MusicOutput for FakeOutput {
        fn live(&self) -> bool {
            self.live
        }
        fn play(&mut self, voice: u32, key: &str, _level: f32, _fade: f32, looping: bool) -> bool {
            self.log.push(format!("play {voice} {key}{}", if looping { " loop" } else { "" }));
            self.voices.insert(voice, (key.to_string(), false));
            true
        }
        fn stop(&mut self, voice: u32, _fade: f32) {
            self.log.push(format!("stop {voice}"));
            self.voices.remove(&voice);
        }
        fn pause(&mut self, voice: u32, fade: f32) {
            self.log.push(format!("pause {voice} {fade}"));
            if let Some(v) = self.voices.get_mut(&voice) {
                v.1 = true;
            }
        }
        fn resume(&mut self, voice: u32, fade: f32) {
            self.log.push(format!("resume {voice} {fade}"));
            if let Some(v) = self.voices.get_mut(&voice) {
                v.1 = false;
            }
        }
        fn active(&self, voice: u32) -> bool {
            self.voices.contains_key(&voice)
        }
        fn position(&self, _voice: u32) -> Option<f64> {
            None
        }
    }

    pub(crate) fn playlist(label: &str, files: &[&str]) -> Directive {
        Directive {
            fade_secs: 1.0,
            ..Directive::new(Source::keyword(label, "test"), Content::Playlist(files.iter().map(|f| f.to_string()).collect()))
        }
    }

    fn frame(dt: f32) -> Inputs {
        Inputs { dt, ..Default::default() }
    }

    fn no_battle() -> Directive {
        panic!("not in combat")
    }

    #[test]
    fn the_same_location_directive_keeps_its_track_and_another_crossfades() {
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(playlist("Explore", &["music\\explore\\a.mp3"])), &mut out);
        music.step(frame(0.5), no_battle, &mut out);
        assert_eq!(out.log, ["play 24 music\\explore\\a.mp3 loop"]);
        music.set(Layer::Location, Some(playlist("Explore", &["music\\explore\\a.mp3"])), &mut out);
        music.step(frame(0.5), no_battle, &mut out);
        assert_eq!(out.log.len(), 1, "MUS-SEL-005: no restart");
        assert_eq!(music.track(Layer::Location, &out).unwrap().1, 0.5, "the clock runs from the frame it started");
        music.set(Layer::Location, Some(playlist("Public", &["music\\public\\p.mp3"])), &mut out);
        music.step(frame(0.5), no_battle, &mut out);
        assert_eq!(out.log[1..], ["stop 24", "play 24 music\\public\\p.mp3 loop"]);
    }

    #[test]
    fn a_track_that_ends_gives_way_to_the_next_of_its_playlist() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(3);
        music.set(Layer::Location, Some(playlist("Explore", &["music\\explore\\a.mp3", "music\\explore\\b.mp3"])), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        let first = music.track(Layer::Location, &out).unwrap().0;
        out.voices.clear(); // the track played to its end
        music.step(frame(0.1), no_battle, &mut out);
        let second = music.track(Layer::Location, &out).unwrap().0;
        assert_ne!(first, second, "MUS-SEL-002 with the shuffle of Q1");
    }

    #[test]
    fn battle_suspends_location_and_holds_for_the_recovery_time() {
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(playlist("Explore", &["music\\explore\\a.mp3"])), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        let battle = Directive { recovery_secs: Some(5.0), ..playlist("Battle", &["music\\battle\\x.mp3"]) };
        let fight = Inputs { dt: 1.0, in_combat: true, ..Default::default() };
        music.step(fight, || battle.clone(), &mut out);
        assert!(out.voices[&24].1, "Location paused under Battle");
        assert!(out.log.contains(&"pause 24 1".to_string()), "Battle crossfades over Location: {:?}", out.log);
        assert_eq!(out.voices[&16].0, "music\\battle\\x.mp3");
        music.step(frame(4.0), no_battle, &mut out);
        assert!(music.directive(Layer::Battle).is_some(), "still within the recovery time");
        music.step(frame(1.5), no_battle, &mut out);
        assert!(music.directive(Layer::Battle).is_none());
        assert!(!out.voices[&24].1, "Location resumes where it was");
        assert_eq!(music.track(Layer::Location, &out).unwrap().1, 1.5, "only the frame it resumed on counts");
    }

    #[test]
    fn a_sting_pauses_battle_and_location_and_ends_after_one_play() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(playlist("Explore", &["music\\explore\\a.mp3"])), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        music.sting(playlist("DefaultDeath", &["music\\special\\death.mp3"]), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        assert_eq!(out.voices[&0], ("music\\special\\death.mp3".to_string(), false), "played once, not looped");
        assert!(out.log.contains(&"play 0 music\\special\\death.mp3".to_string()));
        assert!(out.voices[&24].1);
        out.voices.remove(&0);
        music.step(frame(1.0), no_battle, &mut out);
        assert!(music.directive(Layer::Sting).is_none());
        assert!(!out.voices[&24].1, "Location resumes (MUS-STG-004)");
    }

    #[test]
    fn hold_pauses_every_layer_and_resumes_at_the_same_position() {
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(playlist("Public", &["music\\public\\p.mp3"])), &mut out);
        music.step(frame(0.0), no_battle, &mut out);
        music.step(frame(2.0), no_battle, &mut out);
        let held = Inputs { dt: 3.0, hold: true, ..Default::default() };
        music.step(held, no_battle, &mut out);
        assert!(out.voices.values().all(|v| v.1), "MUS-MIX-002: every voice paused");
        assert!(out.log.contains(&"pause 24 0".to_string()), "a movie cuts the music at once: {:?}", out.log);
        assert!(music.status(&out)[0].starts_with("Hold: on"));
        assert!(music.status(&out)[4].contains("Playing (paused under Hold)"), "{:?}", music.status(&out));
        music.step(frame(0.0), no_battle, &mut out);
        assert!(out.voices.values().all(|v| !v.1));
        assert!(out.log.contains(&"resume 24 0".to_string()));
        assert_eq!(music.track(Layer::Location, &out).unwrap().1, 2.0, "the held seconds did not count");
    }

    #[test]
    fn a_giving_way_script_directive_waits_while_battle_plays_and_a_plain_one_replaces_it() {
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        let battle = playlist("Battle", &["music\\battle\\x.mp3"]);
        let fight = Inputs { dt: 0.1, in_combat: true, ..Default::default() };
        music.set(Layer::Script, Some(Directive { gives_way_to_battle: true, ..playlist("Stream", &["music\\public\\p.mp3"]) }), &mut out);
        music.step(fight, || battle.clone(), &mut out);
        assert!(!out.voices.contains_key(&8), "the streamed track waits (MUS-CMB-004)");
        assert_eq!(out.voices[&16], ("music\\battle\\x.mp3".to_string(), false), "Battle plays");
        music.set(Layer::Script, Some(playlist("Tension", &["music\\tension\\t.mp3"])), &mut out);
        music.step(fight, || battle.clone(), &mut out);
        assert_eq!(out.voices[&8], ("music\\tension\\t.mp3".to_string(), false));
        assert!(out.voices[&16].1, "PlayMusic replaces the Battle music (MUS-SCR-005, #6899)");
        assert!(music.status(&out)[3].contains("paused under Script"), "{:?}", music.status(&out));
    }

    /// #6899 — vanilla scripts rely on PlayMusic replacing what plays: `PlayMusic 1NoMusic`
    /// silences the cell's music (New Vegas `VVault11ExterminationChairSCRIPT`, `VCG00`), and a
    /// scripted track hands back to it when it ends.
    #[test]
    fn a_scripted_track_or_silence_replaces_the_location_music_until_it_ends() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(playlist("Public", &["music\\public\\p.mp3"])), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        music.set(Layer::Script, Some(Directive::new(Source::keyword("1NoMusic", "test"), Content::Silent)), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        assert!(out.voices[&24].1, "1NoMusic silences the cell's music");
        assert!(music.status(&out)[4].contains("paused under Script"), "{:?}", music.status(&out));
        music.set(Layer::Script, Some(playlist("musSCRGoodspringsStinger", &["music\\scr\\s.mp3"])), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        assert!(out.voices[&24].1 && !out.voices[&8].1, "the stinger plays in its place");
        out.voices.remove(&8);
        music.step(frame(0.1), no_battle, &mut out);
        assert!(music.directive(Layer::Script).is_none());
        assert!(!out.voices[&24].1, "the cell's music resumes when the stinger ends");
    }

    #[test]
    fn a_scripted_track_that_ends_hands_back_and_a_silent_one_ends_the_layer() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        music.set(Layer::Script, Some(playlist("musSCRVegasStinger", &["music\\scr\\s.mp3"])), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        assert_eq!(out.log, ["play 8 music\\scr\\s.mp3"], "a scripted single track plays once");
        out.voices.clear();
        music.step(frame(0.1), no_battle, &mut out);
        assert!(music.directive(Layer::Script).is_none());
        music.set(Layer::Script, Some(playlist("MUSVault10106", &["music\\a.mp3"])), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        music.set(Layer::Script, Some(Directive::new(Source::keyword("1NoMusic", "test"), Content::Silent)), &mut out);
        assert!(!out.voices.contains_key(&8), "PlayMusic 1NoMusic stops the scripted track");
        assert!(music.status(&out)[2].starts_with("Script: Silent"), "{:?}", music.status(&out));
    }

    #[test]
    fn stems_follow_day_night_and_depth_into_the_area() {
        use crate::music::directive::{Area, Stem};
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        let stem = |key: &str, night: bool, reach: f32| Stem { key: key.into(), level: 1.0, night, reach };
        let set = Directive {
            looping: true,
            ..Directive::new(
                Source::keyword("musSetGoodsprings", "test"),
                Content::Stems {
                    stems: vec![stem("day_outer", false, 1.0), stem("day_middle", false, 0.666), stem("night_outer", true, 1.0)],
                    area: Area { center: [0.0; 3], radius: 5000.0, day_start: 64, night_start: 255 },
                },
            )
        };
        music.set(Layer::Location, Some(set), &mut out);
        let at = |x: f32, hour: f32| Inputs { dt: 0.1, player: Some([x, 0.0, 0.0]), hour, ..Default::default() };
        music.step(at(4000.0, 12.0), no_battle, &mut out);
        assert_eq!(out.voices.values().map(|v| v.0.as_str()).collect::<Vec<_>>(), ["day_outer"], "outer only, at 80% of the radius");
        music.step(at(1000.0, 12.0), no_battle, &mut out);
        let mut playing: Vec<_> = out.voices.values().map(|v| v.0.as_str()).collect();
        playing.sort_unstable();
        assert_eq!(playing, ["day_middle", "day_outer"], "the middle layer joins deeper in (MUS-SRC-007)");
        music.step(at(1000.0, 2.0), no_battle, &mut out);
        assert_eq!(out.voices.values().map(|v| v.0.as_str()).collect::<Vec<_>>(), ["night_outer"], "night (MUS-SRC-006)");
    }

    #[test]
    fn a_battle_sets_intro_leads_into_its_loop_and_its_outro_follows_the_recovery() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        let battle = Directive {
            recovery_secs: Some(1.0),
            looping: true,
            intro: Some("sound\\intro.wav".into()),
            outro: Some("sound\\outro.wav".into()),
            ..playlist("musSetBattleGS", &["music\\battle\\loop.mp3"])
        };
        let fight = Inputs { dt: 0.1, in_combat: true, ..Default::default() };
        music.step(fight, || battle.clone(), &mut out);
        assert_eq!(out.log, ["play 23 sound\\intro.wav"], "the loop waits for the intro (MUS-CMB-003)");
        out.voices.remove(&23);
        music.step(fight, || battle.clone(), &mut out);
        assert_eq!(out.voices[&16].0, "music\\battle\\loop.mp3");
        music.step(frame(1.5), no_battle, &mut out);
        assert!(music.directive(Layer::Battle).is_none());
        assert_eq!(out.voices[&23].0, "sound\\outro.wav", "the outro as Battle hands back");
    }
}
