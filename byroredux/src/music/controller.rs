//! The game-neutral music controller (`docs/engine/music.md` §2): one directive per layer, what is
//! audible, which track plays, the fades between them, and the report.

use super::directive::{AuthoredTrack, Content, Directive, Outcome, PlaylistPolicy, Shuffle};
use std::collections::HashSet;
use std::sync::Arc;

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

    fn palette_voice(self, slot: u32) -> u32 {
        32 + self as u32 * PALETTE_VOICES + slot
    }
}

const ONE_SHOT_SLOT: u32 = 7;
/// A vanilla palette has up to three layers. Leave room for nested choices
/// while keeping each music layer's share of the audio backend bounded.
const PALETTE_VOICES: u32 = 16;

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
    fn set_level(&mut self, voice: u32, level: f32, fade: f32);
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

#[derive(Debug, Clone, Copy)]
struct PaletteTiming {
    fade_at: f64,
    end_at: f64,
}

impl PaletteTiming {
    fn new(duration: f32, fade_out: f32) -> Self {
        let fade_at = f64::from(duration);
        Self { fade_at, end_at: fade_at + f64::from(fade_out) }
    }

    fn starting_at(self, elapsed: f64) -> Self {
        Self { fade_at: elapsed + self.fade_at, end_at: elapsed + self.end_at }
    }

    fn within(self, parent: Self) -> Self {
        Self { fade_at: self.fade_at.min(parent.fade_at), end_at: self.end_at.min(parent.end_at) }
    }
}

#[derive(Debug)]
struct PaletteVoice {
    playing: Playing,
    fading: bool,
}

#[derive(Debug)]
enum PaletteChoice {
    File(usize),
    Silence(f64),
    Palette(ActivePalette),
}

#[derive(Debug)]
struct PaletteLayer {
    alternatives: Arc<[AuthoredTrack]>,
    current: Option<PaletteChoice>,
    failed: bool,
}

#[derive(Debug)]
struct ActivePalette {
    timing: PaletteTiming,
    layers: Vec<PaletteLayer>,
}

impl ActivePalette {
    fn new(timing: PaletteTiming, layers: &[Arc<[AuthoredTrack]>]) -> Self {
        let layers = layers.iter().map(|alternatives| PaletteLayer {
            alternatives: Arc::clone(alternatives), current: None, failed: false,
        }).collect();
        Self { timing, layers }
    }

    fn step(&mut self, elapsed: f64, output: &mut PaletteOutput<'_>) -> bool {
        for layer in &mut self.layers {
            let ended = match &mut layer.current {
                Some(PaletteChoice::File(slot)) => output.file_ended(*slot, elapsed, self.timing),
                Some(PaletteChoice::Silence(until)) => elapsed >= *until,
                Some(PaletteChoice::Palette(palette)) => palette.step(elapsed, output),
                None => true,
            };
            if ended {
                layer.current = None;
                if !layer.failed && elapsed < self.timing.fade_at {
                    let first = output.shuffle.roll(layer.alternatives.len() as u64) as usize;
                    layer.current = (0..layer.alternatives.len())
                        .map(|offset| &layer.alternatives[(first + offset) % layer.alternatives.len()])
                        .find_map(|track| output.start(track, elapsed, self.timing));
                    layer.failed = layer.current.is_none();
                }
            }
        }
        elapsed >= self.timing.end_at
    }

    fn has_selection(&self) -> bool {
        self.layers.iter().any(|layer| layer.current.is_some())
    }
}

struct PaletteOutput<'a> {
    layer: Layer,
    level: f32,
    fade_in: f32,
    voices: &'a mut [Option<PaletteVoice>; PALETTE_VOICES as usize],
    shuffle: &'a mut Shuffle,
    out: &'a mut dyn MusicOutput,
}

impl PaletteOutput<'_> {
    fn start(&mut self, track: &AuthoredTrack, elapsed: f64, parent: PaletteTiming) -> Option<PaletteChoice> {
        match track {
            AuthoredTrack::File(key) => {
                let slot = self.voices.iter().position(Option::is_none)?;
                let voice = self.layer.palette_voice(slot as u32);
                if !self.out.play(voice, key, self.level, self.fade_in, false) {
                    return None;
                }
                self.voices[slot] = Some(PaletteVoice {
                    playing: Playing { key: key.clone(), voice, clock: 0.0 }, fading: false,
                });
                Some(PaletteChoice::File(slot))
            }
            AuthoredTrack::Silence { duration, .. } => Some(PaletteChoice::Silence(elapsed + f64::from(*duration))),
            AuthoredTrack::Palette { duration, fade_out, layers, .. } => {
                let timing = PaletteTiming::new(*duration, *fade_out).starting_at(elapsed).within(parent);
                let mut palette = ActivePalette::new(timing, layers);
                palette.step(elapsed, self);
                palette.has_selection().then_some(PaletteChoice::Palette(palette))
            }
        }
    }

    fn file_ended(&mut self, slot: usize, elapsed: f64, timing: PaletteTiming) -> bool {
        let Some(voice) = &mut self.voices[slot] else { return true };
        let id = voice.playing.voice;
        if elapsed >= timing.end_at || self.out.live() && !self.out.active(id) {
            self.out.stop(id, 0.0);
            self.voices[slot] = None;
            return true;
        }
        if !voice.fading && elapsed >= timing.fade_at {
            voice.fading = true;
            self.out.set_level(id, 0.0, (timing.end_at - elapsed) as f32);
        }
        false
    }
}

#[derive(Debug)]
struct TimedSelection {
    label: String,
    palette: Option<ActivePalette>,
    timing: PaletteTiming,
    elapsed: f64,
    voices: [Option<PaletteVoice>; PALETTE_VOICES as usize],
}

impl TimedSelection {
    fn step(
        &mut self,
        dt: f32,
        layer: Layer,
        level: f32,
        fade_in: f32,
        shuffle: &mut Shuffle,
        out: &mut dyn MusicOutput,
    ) -> bool {
        self.elapsed += f64::from(dt.max(0.0));
        match &mut self.palette {
            Some(palette) => palette.step(self.elapsed, &mut PaletteOutput { layer, level, fade_in, voices: &mut self.voices, shuffle, out }),
            None => self.elapsed >= self.timing.end_at,
        }
    }

    fn voices(&self) -> impl Iterator<Item = &PaletteVoice> {
        self.voices.iter().flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Ducking {
    by: Layer,
    db: f32,
    fade_secs: f32,
}

impl Ducking {
    fn level(self, original: f32) -> f32 {
        original * 10f32.powf(-self.db / 20.0)
    }
}

#[derive(Clone, Copy)]
enum Mix {
    Full,
    Paused(&'static str),
    Ducked(Ducking),
}

#[derive(Debug, Default)]
struct LayerState {
    /// Shared, so a frame's step reads it without deep-cloning its playlist and notes (#6931).
    directive: Option<Arc<Directive>>,
    /// A playlist's current track.
    track: Option<Playing>,
    /// One timed selection in an authored playlist, with any active palette voices.
    timed: Option<TimedSelection>,
    /// A media set's stems, by stem index, while each plays.
    stems: Vec<Option<Playing>>,
    /// The stems that would not start under this directive, by stem index: tried once, not every
    /// frame (#6921, MUS-SRC-012). A new directive tries them again.
    failed_stems: Vec<bool>,
    /// A battle intro or outro playing on the one-shot slot.
    one_shot: Option<Playing>,
    shuffle: Shuffle,
    /// The next selection within the authored list (or shuffled pass).
    selection: usize,
    selection_started: bool,
    playlist_finished: bool,
    /// The layer (or Hold) that suspends this one, while one does.
    suspended: Option<&'static str>,
    ducking: Option<Ducking>,
    /// The fade for the first track after a directive change, where it is not the directive's own
    /// (`Directive::change_fade_secs`).
    start_fade: Option<f32>,
}

impl LayerState {
    fn outcome(&self) -> Option<Outcome> {
        self.directive.as_ref().map(|directive| {
            if self.playlist_finished { Outcome::Silent } else { directive.outcome() }
        })
    }

    fn level(&self, original: f32) -> f32 {
        self.ducking.map_or(original, |duck| duck.level(original))
    }

    fn voices(&self) -> impl Iterator<Item = &Playing> {
        self.track.iter().chain(self.stems.iter().flatten()).chain(self.one_shot.iter())
            .chain(self.timed.iter().flat_map(|timed| timed.voices().map(|voice| &voice.playing)))
    }

    fn voices_mut(&mut self) -> impl Iterator<Item = &mut Playing> {
        self.track.iter_mut().chain(self.stems.iter_mut().flatten()).chain(self.one_shot.iter_mut())
            .chain(self.timed.iter_mut().flat_map(|timed| timed.voices.iter_mut().flatten().map(|voice| &mut voice.playing)))
    }

    fn stop_all(&mut self, out: &mut dyn MusicOutput, fade: f32) {
        for playing in self.voices() {
            out.stop(playing.voice, fade);
        }
        self.track = None;
        self.timed = None;
        self.stems.clear();
        self.failed_stems.clear();
        self.one_shot = None;
        self.ducking = None;
    }

    /// Start the next playable selection, retaining the authored order or shuffled pass.
    fn start_authored(
        &mut self,
        tracks: &[AuthoredTrack],
        policy: PlaylistPolicy,
        layer: Layer,
        level: f32,
        fade: f32,
        out: &mut dyn MusicOutput,
    ) -> bool {
        let remaining = tracks.len() - self.selection;
        // If a partial pass has no playable entries left, one complete new pass is enough.
        let attempts = if policy.cycle && self.selection != 0 { remaining + tracks.len() } else { remaining };
        for _ in 0..attempts {
            if self.selection == tracks.len() {
                self.selection = 0;
            }
            let index = if policy.ordered { self.selection } else { self.shuffle.next(tracks.len()) };
            self.selection += 1;
            if self.start_selection(&tracks[index], layer, level, fade, out) {
                self.selection_started = true;
                return true;
            }
        }
        self.playlist_finished = self.selection_started && !policy.cycle;
        false
    }

    fn start_selection(
        &mut self,
        track: &AuthoredTrack,
        layer: Layer,
        level: f32,
        fade: f32,
        out: &mut dyn MusicOutput,
    ) -> bool {
        match track {
            AuthoredTrack::File(key) => {
                let voice = layer.voice(0);
                if !out.play(voice, key, level, fade, false) {
                    return false;
                }
                self.track = Some(Playing { key: key.clone(), voice, clock: 0.0 });
            }
            AuthoredTrack::Silence { label, duration } => {
                self.timed = Some(TimedSelection {
                    label: label.clone(), palette: None, timing: PaletteTiming::new(*duration, 0.0),
                    elapsed: 0.0, voices: Default::default(),
                });
            }
            AuthoredTrack::Palette { label, duration, fade_out, layers } => {
                let timing = PaletteTiming::new(*duration, *fade_out);
                let mut timed = TimedSelection {
                    label: label.clone(), palette: Some(ActivePalette::new(timing, layers)), timing,
                    elapsed: 0.0, voices: Default::default(),
                };
                timed.step(0.0, layer, level, fade, &mut self.shuffle, out);
                if *duration > 0.0 && !timed.palette.as_ref().is_some_and(ActivePalette::has_selection) {
                    return false;
                }
                self.timed = Some(timed);
            }
        }
        true
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
    /// A sting stopped the music: Battle and Location stay silent until the place changes, combat
    /// starts or a script plays (MUS-STG-004, #6901).
    silenced: bool,
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
        Self { layers, hold: false, battle_hold: 0.0, silenced: false, reported: HashSet::new() }
    }

    fn state(&self, layer: Layer) -> &LayerState {
        &self.layers[layer as usize]
    }

    #[cfg(test)]
    pub(crate) fn directive(&self, layer: Layer) -> Option<&Directive> {
        self.state(layer).directive.as_deref()
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
        // A new place, combat or scripted track brings back the music a sting stopped (#6901).
        if directive.is_some() && layer != Layer::Sting {
            self.silenced = false;
        }
        let state = &mut self.layers[layer as usize];
        match (&state.directive, &directive) {
            (Some(old), Some(new)) if old.same_music(new) && !(layer == Layer::Script && state.playlist_finished) => {
                state.directive = directive.map(Arc::new);
                return;
            }
            (None, None) => return,
            _ => {}
        }
        let fade = match &directive {
            Some(new) => new.change_fade_secs.unwrap_or(new.fade_secs),
            None => state.directive.as_ref().map_or(0.0, |old| old.fade_secs),
        };
        if directive.as_ref().is_some_and(|d| matches!(d.content, Content::AuthoredPlaylist { .. })) {
            state.shuffle.reset();
        }
        state.stop_all(out, fade);
        state.selection = 0;
        state.selection_started = false;
        state.playlist_finished = false;
        state.start_fade = directive.as_ref().and_then(|d| d.change_fade_secs);
        state.directive = directive.map(Arc::new);
        if let Some(d) = state.directive.clone() {
            self.report(layer, &d);
        }
    }

    /// End a sting's silence: a load or a cell change (#6922). A place change already ends it through
    /// [`Self::set`], but a load back into the same cell or grid changes no place, so the music a
    /// death sting stopped stayed silent until the player left the area (`docs/engine/music.md` §9).
    pub(crate) fn end_sting_silence(&mut self) {
        self.silenced = false;
    }

    /// Whether a sting's silence holds Battle and Location.
    #[cfg(test)]
    pub(crate) fn silenced(&self) -> bool {
        self.silenced
    }

    /// End the Script layer: a cell change or a load (MUS-SCR, MUS-SAV-002). An Oblivion stream is
    /// left to [`Self::place_category`], which runs when the place changes.
    pub(crate) fn end_script(&mut self, out: &mut dyn MusicOutput) {
        if self.state(Layer::Script).directive.as_ref().is_some_and(|d| d.category.is_some()) {
            return;
        }
        self.set(Layer::Script, None, out);
    }

    /// Oblivion's place change (#6903): `0x6ab160` keeps the music whose type is the new place's
    /// and replaces any other, so a stream outlives a change into its own category only.
    pub(crate) fn place_category(&mut self, category: u32, out: &mut dyn MusicOutput) {
        if self.state(Layer::Script).directive.as_ref().and_then(|d| d.category).is_some_and(|own| own != category) {
            self.set(Layer::Script, None, out);
        }
    }

    /// A roll in `0..n` from the Script layer's generator (`StreamMusic Random`).
    pub(crate) fn roll(&mut self, n: u64) -> u64 {
        self.layers[Layer::Script as usize].shuffle.roll(n)
    }

    /// Play a sting (MUS-STG-004, #6901): over the music (Fallout 3 / New Vegas's level-up), or in
    /// its place. Fallout3.exe stops the music on the player's death (`0x8e52b0`); Oblivion.exe
    /// plays both stings on its one music graph, stopping the track (`0x6ab160`), and nothing
    /// comes back until the place changes, combat starts or a script plays: the place's music then
    /// starts afresh.
    pub(crate) fn sting(&mut self, directive: Directive, out: &mut dyn MusicOutput) {
        if !directive.over_music {
            self.set(Layer::Script, None, out);
            for layer in [Layer::Battle, Layer::Location] {
                self.layers[layer as usize].stop_all(out, 0.0);
            }
            self.silenced = true;
        }
        // A second sting restarts the slot rather than being merged into the first.
        self.set(Layer::Sting, None, out);
        self.set(Layer::Sting, Some(directive), out);
    }

    /// Advance one frame: Battle's start and hold, what each layer may play, and which track.
    /// `battle` is asked for the Battle directive only when combat starts.
    pub(crate) fn step(&mut self, inputs: Inputs, battle: impl FnOnce() -> Directive, out: &mut dyn MusicOutput) {
        self.hold = inputs.hold;
        self.step_battle(inputs, battle, out);

        // Battle replaces an Oblivion stream: arriving in combat, or combat starting under it, ends
        // it, and Location follows combat (Oblivion.exe `0x6acd10`, MUS-CMB-004, #6903).
        if self.state(Layer::Battle).outcome() == Some(Outcome::Playing)
            && self.state(Layer::Script).directive.as_ref().is_some_and(|d| d.ends_on_battle)
        {
            self.set(Layer::Script, None, out);
        }
        // Top down, so a layer sees the higher layers as this frame left them (a sting that just
        // ended releases Location the same frame).
        for layer in Layer::ALL {
            let mix = self.mix_for(layer, inputs);
            self.apply_mix(layer, mix, out);
            self.play(layer, inputs, out);
        }
    }

    /// How the current stack affects this layer, including a battle set's intro and outro (#6928).
    fn mix_for(&self, layer: Layer, inputs: Inputs) -> Mix {
        if inputs.hold {
            return Mix::Paused("Hold");
        }
        let playing = |layer: Layer| self.state(layer).outcome() == Some(Outcome::Playing);
        let sting_audible = playing(Layer::Sting) && self.state(Layer::Sting).directive.as_ref().is_some_and(|d| !d.over_music);
        if layer != Layer::Sting && sting_audible {
            return Mix::Paused("Sting");
        }
        if matches!(layer, Layer::Battle | Layer::Location) && self.silenced {
            return Mix::Paused("a sting's silence");
        }
        if layer != Layer::Sting && let Some(winner) = self.music_type_winner() {
            if layer == winner {
                return Mix::Full;
            }
            let current = self.state(layer);
            let leading = self.state(winner).directive.as_ref().expect("winner has a directive");
            let lower = current.directive.as_ref().and_then(|d| d.priority)
                .zip(leading.priority).is_some_and(|(current, leading)| current > leading);
            let already_playing = (current.track.is_some() || current.timed.is_some())
                && (current.suspended.is_none() || current.suspended == Some("Hold"));
            if lower && playing(winner) && already_playing
                && let Some(db) = leading.ducking_db
            {
                return Mix::Ducked(Ducking { by: winner, db, fade_secs: leading.fade_secs });
            }
            return Mix::Paused(winner.name());
        }
        let battle_audible = playing(Layer::Battle);
        // A scripted track replaces the music below it, and a scripted silence (`PlayMusic
        // 1NoMusic`) silences it (MUS-SCR-005, #6899).
        let script = self.state(Layer::Script).directive.as_ref();
        let script_holds = script.is_some_and(|d| d.outcome() == Outcome::Silent) || playing(Layer::Script);
        let beside_script = self.state(Layer::Location).directive.as_ref().is_some_and(|d| d.beside_script);
        match layer {
            Layer::Battle if script_holds => Mix::Paused("Script"),
            Layer::Location if script_holds && !beside_script => Mix::Paused("Script"),
            Layer::Location if battle_audible => Mix::Paused("Battle"),
            _ => Mix::Full,
        }
    }

    fn music_type_winner(&self) -> Option<Layer> {
        // Authored types share a priority stack. Ties retain the controller's layer order.
        [Layer::Script, Layer::Battle, Layer::Location].into_iter()
            .filter_map(|layer| {
                let directive = self.state(layer).directive.as_ref()?;
                if self.state(layer).playlist_finished {
                    return None;
                }
                match directive.outcome() {
                    Outcome::Playing | Outcome::Silent => directive.priority.map(|priority| (priority, layer)),
                    Outcome::Unresolved | Outcome::Missing => None,
                }
            })
            .min_by_key(|(priority, _)| *priority)
            .map(|(_, layer)| layer)
    }

    fn step_battle(&mut self, inputs: Inputs, battle: impl FnOnce() -> Directive, out: &mut dyn MusicOutput) {
        if inputs.in_combat {
            if self.state(Layer::Battle).directive.is_none() {
                let directive = battle();
                self.set(Layer::Battle, Some(directive), out);
                // The intro is Battle's music too: a movie's hold, a sting or a scripted track
                // that holds Battle holds it (#6928).
                if !matches!(self.mix_for(Layer::Battle, inputs), Mix::Paused(_)) {
                    self.one_shot(Layer::Battle, true, out);
                }
            }
            self.battle_hold = self.state(Layer::Battle).directive.as_ref().and_then(|d| d.recovery_secs).unwrap_or(0.0);
        } else if self.state(Layer::Battle).directive.is_some() {
            self.battle_hold -= inputs.dt;
            if self.battle_hold <= 0.0 {
                // A battle set's outro plays as Battle hands back (MUS-CMB-003).
                let outro = self.state(Layer::Battle).directive.as_ref().and_then(|d| d.outro.clone().map(|key| (key, d.level)));
                self.set(Layer::Battle, None, out);
                // Not after a death sting, under a movie, or under a scripted track: the original
                // stops all music there (#6928).
                if let Some((key, level)) = outro.filter(|_| !matches!(self.mix_for(Layer::Battle, inputs), Mix::Paused(_))) {
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

    fn apply_mix(&mut self, layer: Layer, mix: Mix, out: &mut dyn MusicOutput) {
        let by = match mix {
            Mix::Paused(by) => Some(by),
            Mix::Full | Mix::Ducked(_) => None,
        };
        if by.is_none() {
            let ducking = match mix {
                Mix::Ducked(ducking) => Some(ducking),
                _ => None,
            };
            let state = &mut self.layers[layer as usize];
            if ducking != state.ducking {
                let fade = ducking.or(state.ducking).map_or(0.0, |duck| duck.fade_secs);
                state.ducking = ducking;
                if let Some(directive) = &state.directive {
                    let level = state.level(directive.level);
                    let palette = state.timed.iter().flat_map(TimedSelection::voices)
                        .filter(|voice| !voice.fading).map(|voice| &voice.playing);
                    for track in state.track.iter().chain(palette) {
                        out.set_level(track.voice, level, fade);
                    }
                }
            }
        }
        self.suspend(layer, by, out);
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
                    // A legacy sting or script can hand back to Location or to a follow-up directive.
                    if matches!(layer, Layer::Sting | Layer::Script) && !d.persists {
                        state.directive = d.then.as_deref().cloned().map(Arc::new);
                        return;
                    }
                }
                if state.track.is_none() {
                    let voice = layer.voice(0);
                    let fade = state.start_fade.take().unwrap_or(d.fade_secs);
                    let index = state.shuffle.next(tracks.len());
                    let looping = d.looping || tracks.len() == 1 && !matches!(layer, Layer::Sting | Layer::Script);
                    // AUD-3: skip a file that cannot play and try the rest of this playlist.
                    let played = (0..tracks.len())
                        .map(|offset| &tracks[(index + offset) % tracks.len()])
                        .find(|key| out.play(voice, key, d.level, fade, looping));
                    match played {
                        Some(key) => state.track = Some(Playing { key: key.clone(), voice, clock: 0.0 }),
                        None => {
                            let path = tracks.get(index).cloned().unwrap_or_else(|| d.source.label.clone());
                            let missing = Directive { content: Content::Missing(path), ..(*d).clone() };
                            state.directive = Some(Arc::new(missing.clone()));
                            self.report(layer, &missing);
                        }
                    }
                }
            }
            Content::AuthoredPlaylist { tracks, policy } => {
                if state.playlist_finished {
                    return;
                }
                let level = state.level(d.level);
                let timed_ended = state.timed.as_mut().is_some_and(|timed| {
                    timed.step(inputs.dt, layer, level, 0.0, &mut state.shuffle, out)
                });
                if timed_ended {
                    state.timed = None;
                }
                let file_ended = state.track.as_ref().is_some_and(|track| live && !out.active(track.voice));
                if file_ended && let Some(track) = state.track.take() {
                    out.stop(track.voice, 0.0);
                }
                if policy.one_selection && (timed_ended || file_ended) {
                    state.playlist_finished = true;
                    return;
                }
                if state.timed.is_none() && state.track.is_none() {
                    let fade = state.start_fade.take().unwrap_or(d.fade_secs);
                    if !state.start_authored(tracks, *policy, layer, level, fade, out) && !state.playlist_finished {
                        let path = match tracks.first() {
                            Some(AuthoredTrack::File(key)) => key.clone(),
                            _ => d.source.label.clone(),
                        };
                        let missing = Directive { content: Content::Missing(path), ..(*d).clone() };
                        state.directive = Some(Arc::new(missing.clone()));
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
                state.failed_stems.resize(stems.len(), false);
                let start_fade = state.start_fade.take().unwrap_or(d.fade_secs);
                for (i, stem) in stems.iter().enumerate() {
                    let wanted = stem.night != day && depth <= stem.reach;
                    let voice = layer.voice(i as u32);
                    match (&state.stems[i], wanted) {
                        // A stem that would not start stays silent under this directive: retrying it
                        // re-read and re-decoded the file, with a warning, every frame (#6921).
                        (None, true) if state.failed_stems[i] => {}
                        (None, true) => {
                            if out.play(voice, &stem.key, d.level * stem.level, start_fade, true) {
                                state.stems[i] = Some(Playing { key: stem.key.clone(), voice, clock: 0.0 });
                            } else {
                                state.failed_stems[i] = true;
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
            let outcome = match (&d.content, state.suspended, state.ducking) {
                _ if state.playlist_finished => "Silent (selection complete)".to_string(),
                (Content::AuthoredPlaylist { .. }, paused, ducking) if state.timed.as_ref().is_some_and(|timed| timed.voices().next().is_none()) =>
                    match (paused, ducking) {
                        (Some(by), _) => format!("Silent (timed selection, paused under {by})"),
                        (None, Some(duck)) => format!("Silent (timed selection, ducked under {} by {:.1} dB)", duck.by.name(), duck.db),
                        (None, None) => "Silent (timed selection)".to_string(),
                    },
                (Content::Playlist(_) | Content::AuthoredPlaylist { .. } | Content::Stems { .. }, Some(by), _) =>
                    format!("Playing (paused under {by})"),
                (Content::Playlist(_) | Content::AuthoredPlaylist { .. }, None, Some(duck)) =>
                    format!("Playing (ducked under {} by {:.1} dB)", duck.by.name(), duck.db),
                (Content::Playlist(_) | Content::AuthoredPlaylist { .. } | Content::Stems { .. }, None, _) => "Playing".to_string(),
                (Content::Silent, _, _) => "Silent".to_string(),
                (Content::Unresolved(reason), _, _) => format!("Unresolved ({reason})"),
                (Content::Missing(path), _, _) => format!("Missing ({path})"),
            };
            let mut line = format!("{}: {outcome} — {}", layer.name(), d.source);
            for playing in state.voices() {
                let position = out.position(playing.voice).unwrap_or(playing.clock);
                line.push_str(&format!(" — {} at {position:.1}s", playing.key));
            }
            if let Some(timed) = &state.timed {
                let kind = if timed.palette.is_some() { "palette" } else { "silence" };
                line.push_str(&format!(" — {kind} {} {:.1}/{:.1}s", timed.label, timed.elapsed, timed.timing.end_at));
            }
            if layer == Layer::Battle && state.outcome() == Some(Outcome::Playing) && self.battle_hold > 0.0 {
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

    fn authored(label: &str, tracks: Vec<AuthoredTrack>, policy: PlaylistPolicy) -> Directive {
        Directive::new(Source::keyword(label, "test"), Content::AuthoredPlaylist { tracks, policy })
    }

    fn files(keys: &[&str]) -> Vec<AuthoredTrack> {
        keys.iter().map(|key| AuthoredTrack::File((*key).into())).collect()
    }

    #[test]
    fn authored_priority_can_keep_location_audible_or_switch_to_battle() {
        let mut music = MusicController::seeded(1);
        let mut out = FakeOutput::default();
        let authored = |file: &str, priority| Directive {
            priority: Some(priority),
            ..Directive::new(Source::keyword(file, "test"), Content::Playlist(vec![file.into()]))
        };
        music.set(Layer::Location, Some(authored("location", 5)), &mut out);
        let combat = Inputs { in_combat: true, ..Default::default() };
        music.step(combat, || authored("combat", 8), &mut out);
        assert!(music.track(Layer::Battle, &out).is_none());
        assert_eq!(music.state(Layer::Battle).suspended, Some("Location"));

        music.set(Layer::Location, Some(authored("location", 90)), &mut out);
        music.step(combat, || unreachable!("the battle request still exists"), &mut out);
        assert_eq!(music.track(Layer::Battle, &out).unwrap().0, "combat");
        assert_eq!(music.state(Layer::Location).suspended, Some("Battle"));
    }

    #[test]
    fn ducking_keeps_the_same_voice_advancing_and_restores_its_level() {
        let mut music = MusicController::seeded(1);
        let mut out = FakeOutput { live: true, ..Default::default() };
        let location = Directive { priority: Some(90), level: 0.5, ..playlist("exterior", &["explore"]) };
        let battle = Directive {
            priority: Some(8),
            ducking_db: Some(20.0),
            fade_secs: 3.0,
            ..authored("combat", files(&["battle", "extra"]), PlaylistPolicy {
                one_selection: true, cycle: true, ordered: true,
            })
        };
        let ambient = Layer::Location.voice(0);
        music.set(Layer::Location, Some(location), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        let combat = Inputs { in_combat: true, ..frame(1.0) };
        music.step(combat, || battle, &mut out);
        assert_eq!(out.levels, [(ambient, 0.05, 3.0)]);
        assert_eq!(music.track(Layer::Location, &out), Some(("explore".into(), 1.0)));

        music.step(Inputs { hold: true, ..combat }, no_battle, &mut out);
        assert!(out.voices[&ambient].1);
        music.step(combat, no_battle, &mut out);
        assert!(!out.voices[&ambient].1);
        assert_eq!(out.levels.len(), 1);
        assert_eq!(music.track(Layer::Location, &out), Some(("explore".into(), 2.0)));

        // Completing the winning selection releases the duck even if combat itself continues.
        out.voices.remove(&Layer::Battle.voice(0));
        music.step(combat, no_battle, &mut out);
        assert_eq!(out.levels.last(), Some(&(ambient, 0.5, 3.0)));
        assert_eq!(music.track(Layer::Location, &out), Some(("explore".into(), 3.0)));
        assert_eq!(out.log.iter().filter(|line| line.starts_with(&format!("play {ambient} "))).count(), 1);
        assert_eq!(music.state(Layer::Battle).outcome(), Some(Outcome::Silent));
        assert!(music.track(Layer::Battle, &out).is_none());
    }

    #[test]
    fn authored_playlists_follow_order_cycle_and_one_selection_independently() {
        let tracks = ["a", "b", "c"];
        for (ordered, cycle, one_selection, expected) in [
            (true, false, false, vec!["a", "b", "c"]),
            (true, true, false, vec!["a", "b", "c", "a", "b"]),
            (true, true, true, vec!["a"]),
            (true, false, true, vec!["a"]),
            (false, false, false, vec!["a", "b", "c"]),
            (false, true, false, vec!["a", "b", "c", "a", "b"]),
        ] {
            let policy = PlaylistPolicy { ordered, cycle, one_selection };
            let authored = Directive {
                priority: Some(90), ..authored("exterior", files(&tracks), policy)
            };
            let mut music = MusicController::seeded(1);
            let mut out = FakeOutput { live: true, ..Default::default() };
            music.set(Layer::Location, Some(authored.clone()), &mut out);
            let voice = Layer::Location.voice(0);
            let mut actual = Vec::new();
            for _ in 0..5 {
                music.step(frame(1.0), no_battle, &mut out);
                if let Some((key, _)) = out.voices.remove(&voice) {
                    actual.push(key);
                }
            }
            assert_eq!(actual.len(), expected.len(), "{policy:?}");
            if ordered {
                assert_eq!(actual, expected, "{policy:?}");
            } else {
                let mut first_pass = actual[..3].to_vec();
                first_pass.sort();
                assert_eq!(first_pass, tracks, "{policy:?}");
                if cycle {
                    assert_ne!(actual[2], actual[3]);
                }
            }
            assert!(out.log.iter().all(|line| !line.ends_with(" loop")));
            if one_selection || !cycle {
                music.set(Layer::Location, Some(authored), &mut out);
                music.step(frame(1.0), no_battle, &mut out);
                assert!(out.voices.is_empty(), "an unchanged location keeps its completed playlist");
                assert!(music.status(&out).iter().any(|line| line.contains("Silent (selection complete)")));
            }
        }
    }

    #[test]
    fn authored_silence_advances_after_its_duration_and_freezes_during_hold() {
        let mut music = MusicController::seeded(1);
        let mut out = FakeOutput { live: true, ..Default::default() };
        let tracks = vec![AuthoredTrack::Silence { label: "rest".into(), duration: 2.0 }, AuthoredTrack::File("song".into())];
        let policy = PlaylistPolicy { one_selection: false, cycle: false, ordered: true };
        music.set(Layer::Location, Some(authored("exterior", tracks, policy)), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        assert!(out.voices.is_empty());
        music.step(frame(1.0), no_battle, &mut out);
        music.step(Inputs { hold: true, ..frame(5.0) }, no_battle, &mut out);
        assert!(music.status(&out).iter().any(|line| line.contains("silence rest 1.0/2.0s")));
        music.step(frame(1.0), no_battle, &mut out);
        assert_eq!(music.track(Layer::Location, &out), Some(("song".into(), 0.0)));
    }

    #[test]
    fn a_palette_starts_all_selected_layers_and_fades_as_one_selection() {
        let mut music = MusicController::seeded(1);
        let mut out = FakeOutput { live: true, ..Default::default() };
        let palette = AuthoredTrack::Palette {
            label: "ensemble".into(), duration: 3.0, fade_out: 2.0,
            layers: vec![
                files(&["a", "b"]).into(),
                files(&["c"]).into(),
                vec![AuthoredTrack::Silence { label: "rest".into(), duration: 1.0 }].into(),
            ],
        };
        let policy = PlaylistPolicy { one_selection: true, cycle: true, ordered: true };
        let tracks = vec![palette, AuthoredTrack::File("extra".into())];
        music.set(Layer::Location, Some(authored("exterior", tracks, policy)), &mut out);
        music.step(frame(0.0), no_battle, &mut out);
        let first = Layer::Location.palette_voice(0);
        let second = Layer::Location.palette_voice(1);
        assert_eq!(out.voices.len(), 2);
        assert!(matches!(out.voices[&first].0.as_str(), "a" | "b"));
        assert_eq!(out.voices[&second].0, "c");
        assert!(out.levels.is_empty());
        out.voices.remove(&first);
        music.step(frame(1.0), no_battle, &mut out);
        assert!(matches!(out.voices[&first].0.as_str(), "a" | "b"));
        assert_eq!(out.voices[&second].0, "c");
        music.step(frame(2.0), no_battle, &mut out);
        assert_eq!(out.levels, [(first, 0.0, 2.0), (second, 0.0, 2.0)]);
        music.step(frame(1.0), no_battle, &mut out);
        assert_eq!(out.voices.len(), 2);
        music.step(frame(1.0), no_battle, &mut out);
        assert!(out.voices.is_empty());
        assert!(music.state(Layer::Location).playlist_finished);
        assert_eq!(out.log.iter().filter(|event| event.starts_with("play ")).count(), 3);
    }

    /// AUD-3 — one track that will not play does not silence its playlist: the next one plays, and
    /// only a playlist with no playable track is Missing.
    #[test]
    fn an_unplayable_track_is_skipped_for_the_next() {
        let mut out = FakeOutput { unplayable: ["broken".to_string()].into(), ..Default::default() };
        let mut music = MusicController::seeded(1);
        let playlist = |files: &[&str]| {
            Directive::new(Source::keyword("explore", "test"), Content::Playlist(files.iter().map(|f| f.to_string()).collect()))
        };
        let at = Inputs { dt: 0.1, player: Some([0.0; 3]), hour: 12.0, ..Default::default() };
        for _ in 0..4 {
            music.set(Layer::Location, Some(playlist(&["broken", "fine"])), &mut out);
            music.step(at, no_battle, &mut out);
            assert_eq!(out.voices.values().map(|v| v.0.as_str()).collect::<Vec<_>>(), ["fine"], "{:?}", out.log);
            music.set(Layer::Location, None, &mut out);
            music.step(at, no_battle, &mut out);
        }
        assert!(out.log.iter().any(|line| line.contains(" broken")), "the shuffle tried the broken track: {:?}", out.log);
        let mut out = FakeOutput { unplayable: ["broken".to_string()].into(), ..Default::default() };
        music.set(Layer::Location, Some(playlist(&["broken"])), &mut out);
        music.step(at, no_battle, &mut out);
        assert!(out.voices.is_empty(), "nothing plays");
    }

    /// A recording output: every voice "plays" until a test ends it.
    #[derive(Default)]
    pub(crate) struct FakeOutput {
        pub(crate) live: bool,
        pub(crate) voices: HashMap<u32, (String, bool)>,
        pub(crate) log: Vec<String>,
        /// `play` / `stop` with their fades.
        pub(crate) faded: Vec<String>,
        pub(crate) levels: Vec<(u32, f32, f32)>,
        /// Keys whose `play` fails (a file that cannot be read or decoded).
        pub(crate) unplayable: std::collections::HashSet<String>,
    }

    impl MusicOutput for FakeOutput {
        fn live(&self) -> bool {
            self.live
        }
        fn play(&mut self, voice: u32, key: &str, _level: f32, fade: f32, looping: bool) -> bool {
            self.log.push(format!("play {voice} {key}{}", if looping { " loop" } else { "" }));
            if self.unplayable.contains(key) {
                return false;
            }
            self.faded.push(format!("play {voice} {fade}"));
            self.voices.insert(voice, (key.to_string(), false));
            true
        }
        fn stop(&mut self, voice: u32, fade: f32) {
            self.log.push(format!("stop {voice}"));
            self.faded.push(format!("stop {voice} {fade}"));
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
        fn set_level(&mut self, voice: u32, level: f32, fade: f32) {
            self.levels.push((voice, level, fade));
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

    /// #6901 — a sting in the music's place (Fallout 3 / New Vegas death, either Oblivion sting)
    /// stops it, and the place's music returns afresh only with the next place, combat or script.
    #[test]
    fn a_sting_stops_the_music_until_the_place_changes() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        let explore = playlist("Explore", &["music\\explore\\a.mp3"]);
        music.set(Layer::Location, Some(explore.clone()), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        music.sting(playlist("death", &["music\\special\\death.mp3"]), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        assert_eq!(out.voices[&0], ("music\\special\\death.mp3".to_string(), false), "played once, not looped");
        assert!(!out.voices.contains_key(&24), "the music is stopped, not paused");
        out.voices.remove(&0);
        music.step(frame(1.0), no_battle, &mut out);
        assert!(music.directive(Layer::Sting).is_none());
        music.step(frame(1.0), no_battle, &mut out);
        assert!(!out.voices.contains_key(&24), "nothing comes back on its own");
        assert!(music.status(&out)[4].contains("a sting's silence"), "{:?}", music.status(&out));
        music.set(Layer::Location, Some(explore), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        assert_eq!(out.voices[&24], ("music\\explore\\a.mp3".to_string(), false), "the next place's music starts afresh");
    }

    /// #6922 — a load back into the same cell changes no place, but it still ends a sting's
    /// silence: the place's music plays again.
    #[test]
    fn a_load_into_the_same_place_ends_a_stings_silence() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(playlist("Explore", &["music\\explore\\a.mp3"])), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        music.sting(playlist("death", &["music\\special\\death.mp3"]), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        out.voices.remove(&0);
        music.step(frame(1.0), no_battle, &mut out);
        assert!(!out.voices.contains_key(&24), "silenced by the sting");
        music.end_sting_silence();
        music.step(frame(1.0), no_battle, &mut out);
        assert_eq!(out.voices[&24].0, "music\\explore\\a.mp3", "the same place's music plays again");
    }

    /// #6901 — Fallout 3 / New Vegas's level-up `MUSSuccess` plays over the music.
    #[test]
    fn a_sting_over_the_music_leaves_it_playing() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(playlist("Explore", &["music\\explore\\a.mp3"])), &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        music.sting(Directive { over_music: true, ..playlist("MUSSuccess", &["sound\\fx\\mus\\mus_success.wav"]) }, &mut out);
        music.step(frame(1.0), no_battle, &mut out);
        assert_eq!(out.voices[&0].0, "sound\\fx\\mus\\mus_success.wav");
        assert_eq!(out.voices[&24], ("music\\explore\\a.mp3".to_string(), false), "the music plays on");
        out.voices.remove(&0);
        music.step(frame(1.0), no_battle, &mut out);
        assert!(music.directive(Layer::Sting).is_none());
        assert_eq!(music.track(Layer::Location, &out).unwrap().1, 2.0, "never paused: every frame after the first counts");
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

    /// #6903 — Oblivion.exe's `0x6acd10`: combat replaces a stream, which does not come back.
    /// #6946 — Fallout3.exe drops a `PlayMusic` while Battle plays, the same flag. A Script
    /// directive Battle does not end (no game's translation makes one now) replaces Battle instead.
    #[test]
    fn battle_ends_a_stream_or_a_play_music_track_and_an_unflagged_one_replaces_battle() {
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        let battle = playlist("Battle", &["music\\battle\\x.mp3"]);
        let fight = Inputs { dt: 0.1, in_combat: true, ..Default::default() };
        music.set(Layer::Script, Some(Directive { ends_on_battle: true, ..playlist("Stream", &["music\\public\\p.mp3"]) }), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        assert!(out.voices.contains_key(&8), "the stream plays out of combat");
        music.step(fight, || battle.clone(), &mut out);
        assert!(music.directive(Layer::Script).is_none(), "combat ends the stream (MUS-CMB-004)");
        assert!(!out.voices.contains_key(&8));
        assert_eq!(out.voices[&16], ("music\\battle\\x.mp3".to_string(), false), "Battle plays");
        music.set(Layer::Script, Some(Directive { ends_on_battle: true, ..playlist("MUSDefaultBattle", &["music\\b\\t.mp3"]) }), &mut out);
        music.step(fight, || battle.clone(), &mut out);
        assert!(music.directive(Layer::Script).is_none(), "a PlayMusic under Battle is dropped");
        assert!(!out.voices.contains_key(&8) && !out.voices[&16].1, "Battle plays on: {:?}", out.voices);
        music.set(Layer::Script, Some(playlist("Tension", &["music\\tension\\t.mp3"])), &mut out);
        music.step(fight, || battle.clone(), &mut out);
        assert_eq!(out.voices[&8], ("music\\tension\\t.mp3".to_string(), false));
        assert!(out.voices[&16].1, "an unflagged Script directive replaces the Battle music");
        assert!(music.status(&out)[3].contains("paused under Script"), "{:?}", music.status(&out));
    }

    /// #6903 — Oblivion.exe's cell change (`0x6ab160`): another category's music starts at once; a
    /// stream outlives the change into its own category and ends otherwise, not on the cell leave.
    #[test]
    fn an_oblivion_place_change_switches_at_once_and_a_stream_outlives_its_own_category() {
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        let place = |label: &str, file: &str, category| Directive {
            fade_secs: 0.85,
            change_fade_secs: Some(0.0),
            category: Some(category),
            ..playlist(label, &[file])
        };
        music.set(Layer::Location, Some(place("Public", "music\\public\\p.mp3", 1)), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        music.set(Layer::Location, Some(place("Dungeon", "music\\dungeon\\d.mp3", 2)), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        assert_eq!(out.faded, ["play 24 0", "stop 24 0", "play 24 0"], "a cell change switches at once");
        let stream = Directive { category: Some(2), persists: true, ..playlist("Dungeon", &["music\\dungeon\\e.mp3"]) };
        music.set(Layer::Script, Some(stream), &mut out);
        music.end_script(&mut out);
        music.place_category(2, &mut out);
        assert!(music.directive(Layer::Script).is_some(), "a Dungeon stream outlives a change into a Dungeon cell");
        music.place_category(1, &mut out);
        assert!(music.directive(Layer::Script).is_none(), "and ends in a Public one");
    }

    /// #6903 — after its track, a `StreamMusic` keyword plays another of its folder (Oblivion.exe's
    /// update replays the current type), and a streamed file hands over to the Default music.
    #[test]
    fn an_oblivion_stream_keyword_plays_on_and_a_streamed_file_hands_over_to_default() {
        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        let keyword = Directive { persists: true, ..playlist("Dungeon", &["music\\dungeon\\a.mp3", "music\\dungeon\\b.mp3"]) };
        music.set(Layer::Script, Some(keyword), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        let first = out.voices[&8].0.clone();
        out.voices.remove(&8);
        music.step(frame(0.1), no_battle, &mut out);
        assert_ne!(out.voices[&8].0, first, "the next track of the keyword's folder");
        let default = Directive { persists: true, category: Some(0), ..playlist("Default", &["music\\explore\\x.mp3"]) };
        let file = Directive { then: Some(Box::new(default)), category: Some(0), ..playlist("file", &["music\\special\\s.mp3"]) };
        music.set(Layer::Script, Some(file), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        out.voices.remove(&8);
        music.step(frame(0.1), no_battle, &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        assert_eq!(music.directive(Layer::Script).map(|d| d.source.label.as_str()), Some("Default"));
        assert_eq!(out.voices[&8].0, "music\\explore\\x.mp3");
    }

    /// #6902 — a New Vegas music marker's music plays on beside a scripted track (GECK wiki
    /// *PlayMusic*: "Will not stop the currently playing music of a music marker. NV Only.").
    #[test]
    fn a_scripted_track_plays_beside_a_music_markers_music() {
        let mut out = FakeOutput::default();
        let mut music = MusicController::seeded(1);
        music.set(Layer::Location, Some(Directive { beside_script: true, ..playlist("musSetGoodsprings", &["music\\loc\\a.mp3"]) }), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        music.set(Layer::Script, Some(playlist("musSCRGoodspringsStinger", &["music\\scr\\s.mp3"])), &mut out);
        music.step(frame(0.1), no_battle, &mut out);
        assert!(!out.voices[&24].1 && !out.voices[&8].1, "both play: {:?}", out.voices);
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

    /// #6921 — a stem whose file will not start is tried once under its directive, not re-read,
    /// re-decoded and re-warned every frame; the other stems play; a new directive tries again.
    #[test]
    fn a_stem_that_will_not_start_is_tried_once_per_directive() {
        use crate::music::directive::{Area, Stem};
        let mut out = FakeOutput { unplayable: ["broken".to_string()].into(), ..Default::default() };
        let mut music = MusicController::seeded(1);
        let stem = |key: &str| Stem { key: key.into(), level: 1.0, night: false, reach: 1.0 };
        let set = |label: &str| Directive {
            looping: true,
            ..Directive::new(
                Source::keyword(label, "test"),
                Content::Stems {
                    stems: vec![stem("broken"), stem("fine")],
                    area: Area { center: [0.0; 3], radius: 5000.0, day_start: 64, night_start: 255 },
                },
            )
        };
        let at = Inputs { dt: 0.1, player: Some([0.0; 3]), hour: 12.0, ..Default::default() };
        music.set(Layer::Location, Some(set("musSetA")), &mut out);
        for _ in 0..5 {
            music.step(at, no_battle, &mut out);
        }
        let tries = |out: &FakeOutput| out.log.iter().filter(|line| line.ends_with(" broken loop")).count();
        assert_eq!(tries(&out), 1, "tried once: {:?}", out.log);
        assert_eq!(out.voices.values().map(|v| v.0.as_str()).collect::<Vec<_>>(), ["fine"]);

        music.set(Layer::Location, Some(set("musSetB")), &mut out);
        music.step(at, no_battle, &mut out);
        assert_eq!(tries(&out), 2, "a new directive tries it again");
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

    /// #6928 — a battle set's intro and outro are Battle's music, held by what holds Battle: the
    /// outro does not play after a death sting, or under a movie's hold; nor the intro under a hold.
    #[test]
    fn a_battle_sets_intro_and_outro_obey_what_holds_battle() {
        let battle = Directive {
            recovery_secs: Some(1.0),
            looping: true,
            intro: Some("sound\\intro.wav".into()),
            outro: Some("sound\\outro.wav".into()),
            ..playlist("musSetBattleGS", &["music\\battle\\loop.mp3"])
        };
        let fight = Inputs { dt: 0.1, in_combat: true, ..Default::default() };

        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        music.step(fight, || battle.clone(), &mut out);
        out.voices.remove(&23);
        music.sting(playlist("death", &["music\\special\\death.mp3"]), &mut out);
        music.step(frame(1.5), no_battle, &mut out);
        assert!(music.directive(Layer::Battle).is_none(), "Battle hands back");
        assert!(!out.log.iter().any(|line| line.contains("outro")), "no outro after a death sting: {:?}", out.log);

        let mut out = FakeOutput { live: true, ..Default::default() };
        let mut music = MusicController::seeded(1);
        music.step(Inputs { hold: true, ..fight }, || battle.clone(), &mut out);
        assert!(!out.log.iter().any(|line| line.contains("intro")), "no intro under a hold: {:?}", out.log);
        music.step(Inputs { dt: 1.5, hold: true, ..Default::default() }, no_battle, &mut out);
        assert!(music.directive(Layer::Battle).is_none());
        assert!(!out.log.iter().any(|line| line.contains("outro")), "no outro under a hold: {:?}", out.log);
    }
}
