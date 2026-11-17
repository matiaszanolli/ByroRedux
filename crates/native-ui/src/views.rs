//! The state the HUD and the native pages draw and the actions they hand back (#6578; moved out of
//! the operator overlay crate by GUI-PAGE-006).

use byroredux_core::settings::{SettingChange, SettingEntry};

/// One script message box (`Message.Show` on a `MESG` with its "Message
/// Box" flag): title, formatted text, and the authored buttons.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageBoxView {
    pub title: String,
    pub text: String,
    pub buttons: Vec<String>,
}

/// One character-creation screen a script opened (`ShowRaceMenu`, `SetTagSkills`, the
/// Vit-o-matic's `ShowLoveTesterMenuParams`, …), with the player's current choices.
#[derive(Debug, Clone, PartialEq)]
pub enum ChargenScreenView {
    /// The player's name (TextEdit, 1051).
    Name,
    /// Sex and race (RaceSex, 1036).
    RaceSex,
    /// SPECIAL: spread `points` over the attributes, each within `min..=max`.
    Special { points: i32, min: i32, max: i32 },
    /// Tag exactly `count` skills.
    TagSkills { count: usize },
    /// Take up to `max` traits.
    Traits { max: usize },
}

/// One choice on a character-creation screen: a race, an attribute, a skill or a trait.
/// `id` is the race or perk FormID, or the actor-value code.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChargenOption {
    pub id: u32,
    pub name: String,
    pub description: String,
}

/// The character-creation screen to draw and what the player has now.
#[derive(Debug, Clone, PartialEq)]
pub struct ChargenSnapshot {
    pub screen: ChargenScreenView,
    /// Changes with every screen opened, so the page seeds its draft once per screen.
    pub serial: u64,
    /// The screen's title, from the game's settings where it has one.
    pub title: String,
    pub name: String,
    pub female: bool,
    pub race: u32,
    pub races: Vec<ChargenOption>,
    /// The attributes with their current values.
    pub attributes: Vec<(ChargenOption, i32)>,
    /// The skills with their current values.
    pub skills: Vec<(ChargenOption, i32)>,
    /// The tagged skills' actor-value codes.
    pub tagged: Vec<u32>,
    pub traits: Vec<ChargenOption>,
    /// The traits the player has, by perk FormID.
    pub chosen_traits: Vec<u32>,
}

/// What the player accepted on a character-creation screen.
#[derive(Debug, Clone, PartialEq)]
pub enum ChargenChoice {
    Name(String),
    RaceSex { female: bool, race: u32 },
    /// `(actor-value code, value)` per attribute.
    Special(Vec<(u32, i32)>),
    /// The tagged skills' actor-value codes.
    TagSkills(Vec<u32>),
    /// The traits taken, by perk FormID.
    Traits(Vec<u32>),
}

/// One barter session: the merchant's goods at the player's buying price and
/// the player's goods the merchant buys, at the selling price.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BarterSnapshot {
    pub vendor_name: String,
    pub player_gold: u32,
    pub vendor_gold: u32,
    pub buy: Vec<BarterItemView>,
    pub sell: Vec<BarterItemView>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BarterItemView {
    pub form_id: u32,
    pub name: String,
    pub count: u32,
    pub price: u32,
}

/// One unit bought from or sold to the merchant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarterAction {
    Buy { form_id: u32 },
    Sell { form_id: u32 },
}

/// One recipe a workbench offers, with what the player holds of each ingredient.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CraftingRecipeView {
    pub form_id: u32,
    /// The item it makes.
    pub name: String,
    /// How many it yields.
    pub count: u32,
    /// `(ingredient name, needed, held)`.
    pub ingredients: Vec<(String, u32, u32)>,
    /// The player holds every ingredient.
    pub craftable: bool,
}

/// The crafting page: the bench's name and its recipes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CraftingSnapshot {
    pub bench_name: String,
    pub recipes: Vec<CraftingRecipeView>,
}

/// The crafting page's mutation: craft one recipe at the open bench.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraftingAction {
    Craft { recipe: u32 },
}

/// Native prompt assembled from the active binding and the selected interaction's verb. The
/// verb is the load order's word (GUI-PAGE-005), shared with the resolved `UiLabels`, so a
/// frame rebuild only bumps a refcount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionPrompt {
    pub binding: &'static str,
    pub verb: std::sync::Arc<str>,
}

/// One vitals bar's presentation state: the composed current value and the undamaged maximum
/// of one canonical actor value. The label is the load order's word (shared with `UiLabels`);
/// the bar's colour keys on `actor_value`, never on the label's spelling (#6825).
#[derive(Debug, Clone, PartialEq)]
pub struct VitalBarView {
    pub label: std::sync::Arc<str>,
    /// The canonical actor value's name (`Health`, `ActionPoints`, `RadiationRads` …).
    pub actor_value: &'static str,
    pub current: f32,
    pub max: f32,
}

/// The detection indicator's presentation (SNK-HUD-001, -002): the state the
/// detection tick folded from every counted detector, drawn in the game's
/// vocabulary. The label is the load order's string, shared with the resolved
/// `StealthIndicator` resource, so a frame rebuild only bumps a refcount.
#[derive(Debug, Clone, PartialEq)]
pub struct StealthIndicatorView {
    pub state: byroredux_core::stealth::IndicatorState,
    pub look: StealthIndicatorLook,
}

/// How the indicator is drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum StealthIndicatorLook {
    /// The game's label for the state (`sSneakHidden` …); the danger state flashes.
    Label(std::sync::Arc<str>),
    /// Oblivion's eye, lit above the hidden state.
    Eye,
}

/// One active quest objective's HUD line: the quest's display name and the
/// objective's authored journal text. Unlike the vitals labels these are
/// authored strings, bounded by the producer's line cap.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectiveView {
    pub quest: String,
    pub text: String,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InventorySnapshot {
    pub items: Vec<InventoryItemView>,
    pub total_weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryItemView {
    pub index: u32,
    pub form_id: u32,
    pub name: String,
    pub category: &'static str,
    pub details: String,
    pub count: u32,
    pub value: u32,
    pub weight: f32,
    pub equipped: bool,
    pub equippable: bool,
    pub consumable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryAction {
    ToggleEquip { index: u32 },
    Consume { index: u32, form_id: u32 },
    /// Drop one of the row's items into the world at the player's feet.
    Drop { index: u32, form_id: u32 },
}

/// One selectable row of the dialogue surface's topic list — the DIALs the
/// NPC owns through a running quest, captured at selection time.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueTopicEntryView {
    pub topic_form_id: u32,
    pub name: String,
    /// The NPC has already said this entry's line (the spoken-set); the page marks it.
    pub said: bool,
}

/// P4 blocker 2 — the activation-selected dialogue topic plus the NPC's
/// whole owned-topic list, populated only while the native dialogue page is
/// visible. Plain data: `native-ui` cannot depend on the binary's component
/// types (same boundary as `InventorySnapshot`).
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueTopicSnapshot {
    /// The speaking NPC's entity id — echoed back in every topic click.
    pub npc: u32,
    /// The speaker's authored full name (`FULL` of its NPC record); empty when it authors none.
    pub speaker_name: String,
    pub topic_form_id: u32,
    pub topic_name: String,
    pub info_form_id: u32,
    pub response_text: String,
    pub topics: Vec<DialogueTopicEntryView>,
    /// The line is still being presented (voice playing, or the subtitle's estimated duration running).
    pub speaking: bool,
    /// The line has real voice audio, not just a subtitle.
    pub voiced: bool,
    /// `topics` are the selected line's own replies (the CK's *choices*), not the general topic menu.
    pub choices: bool,
}

/// The dialogue surface's mutations, applied to canonical world state after
/// egui drops its read-only frame snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogueUiAction {
    /// Re-run the topic selection for one owned topic on the same NPC.
    SelectTopic { npc: u32, topic_form_id: u32 },
}

/// The player-facing state the HUD and the native pages draw, built each frame by the binary.
/// The operator overlay's `PanelSnapshot` carries one as its `game` field.
#[derive(Default, Clone)]
pub struct PageSnapshot {
    /// Original game's LSCR tip while its loading artwork is composited.
    pub loading_tip: Option<String>,
    /// Native in-world HUD prompt. Unlike the debug panels, this remains
    /// visible when the F3 operator overlay is closed. Rebuilt every frame,
    /// including the overlay-hidden path, without allocating (#2680): the
    /// binding is a compile-time constant and the verb a shared `Arc<str>`
    /// from `UiLabels` (GUI-PAGE-005).
    pub interaction_prompt: Option<InteractionPrompt>,
    /// Whether the native reticle should be drawn while gameplay owns input.
    pub show_crosshair: bool,
    /// Whether contextual interaction prompts should be drawn.
    pub show_prompts: bool,
    /// Whether the player's vitals bars should be drawn while gameplay owns
    /// input (`interface.show_vitals`).
    pub show_vitals: bool,
    /// Player vitals bars, rebuilt every frame from the player's canonical
    /// `ActorValues` — like the interaction prompt, this stays live while the
    /// F3 overlay is hidden. `None` (or an empty vec) means no player body or
    /// no resolvable vitals, and the HUD draws nothing.
    pub vitals: Option<Vec<VitalBarView>>,
    /// Whether active quest objective text should be drawn (`interface.show_objectives`).
    pub show_objectives: bool,
    /// Active quest objective lines, rebuilt every frame from canonical quest
    /// state. `None` (or empty) means no running quest has a displayed,
    /// unfinished objective.
    pub objectives: Option<Vec<ObjectiveView>>,
    /// M65 (SNK-HUD-001) — the detection indicator, present only while the
    /// player is in the sneak stance (and, on Starfield, holds the Stealth
    /// skill). Like the prompt, it stays live with the F3 overlay hidden.
    pub stealth_indicator: Option<StealthIndicatorView>,
    /// Deterministically ordered clone of the universal settings registry.
    /// Settings are small and only cloned while the overlay is visible.
    pub settings: Vec<SettingEntry>,
    /// Player inventory, populated only while the native inventory page is
    /// visible. `None` means the current scene has no character player.
    pub inventory: Option<InventorySnapshot>,
    /// The activation-selected dialogue topic + owned-topic list, populated
    /// only while the native dialogue page is visible.
    pub dialogue: Option<DialogueTopicSnapshot>,
    /// The front of the script message-box queue, populated while the native
    /// message-box page is visible.
    pub message_box: Option<MessageBoxView>,
    /// The open barter session, populated while one is in progress.
    pub barter: Option<BarterSnapshot>,
    /// The open crafting bench, populated while its page is up.
    pub crafting: Option<CraftingSnapshot>,
    /// The character-creation screen a script opened, populated while it is up.
    pub chargen: Option<ChargenSnapshot>,
    /// M48 GUI-PAGE-005 — the native pages' words in the load order's vocabulary (a handle;
    /// the default is the engine text).
    pub labels: byroredux_core::ui_labels::UiLabels,
}

impl PageSnapshot {
    /// Whether the snapshot carries a HUD layer that draws with the F3
    /// overlay and the game menu both hidden. The overlay host skips the
    /// whole egui pass when it doesn't, so every always-live layer `draw_hud`
    /// paints must be a term here. Vitals and objectives count only with
    /// content to draw: a toggle with no player body or no active objective
    /// must keep the hidden-overlay steady state cheap.
    pub fn has_live_hud_content(&self) -> bool {
        self.interaction_prompt.is_some()
            || self.show_crosshair
            || (self.show_vitals && self.vitals.as_ref().is_some_and(|v| !v.is_empty()))
            || (self.show_objectives && self.objectives.as_ref().is_some_and(|o| !o.is_empty()))
            || self.loading_tip.is_some()
            // #7002 — the sneak indicator stays live with the overlay hidden.
            || self.stealth_indicator.is_some()
    }
}

/// What the player asked of the game on the HUD and the native pages, applied by the binary after
/// the frame. The operator overlay's `PanelOutputs` carries one as its `game` field.
#[derive(Default, Clone)]
pub struct PageOutputs {
    /// Validated by the universal registry after the egui frame completes.
    pub setting_changes: Vec<SettingChange>,
    /// Mutations applied to canonical player inventory state after egui drops
    /// its read-only frame snapshot.
    pub inventory_actions: Vec<InventoryAction>,
    /// Dialogue-surface mutations (topic clicks) applied through the same
    /// selection the activation path uses.
    pub dialogue_actions: Vec<DialogueUiAction>,
    /// The button the player chose on the visible message box (its index;
    /// 0 for the implicit "Ok" of a box without buttons).
    pub message_box_choice: Option<usize>,
    /// Trades the player made on the barter page, applied in order.
    pub barter_actions: Vec<BarterAction>,
    /// Recipes the crafting page asked to craft.
    pub crafting_actions: Vec<CraftingAction>,
    /// What the player accepted on the open character-creation screen.
    pub chargen_choice: Option<ChargenChoice>,
    /// Close the native pause menu and return input to the world.
    pub resume_game: bool,
    /// Save to the next ring slot through the binary's canonical save API.
    pub quicksave: bool,
    /// Load the most recently written slot through the canonical load queue.
    pub quickload: bool,
    /// Perform the same orderly shutdown as the window close button.
    pub quit_game: bool,
}

#[cfg(test)]
mod tests {
    /// #7002 — every always-live HUD layer keeps the egui pass running with
    /// the overlay hidden. The sneak indicator was drawn by `draw_hud` but was
    /// no term of the skip, so with the crosshair, vitals and objectives off a
    /// sneaking player's label never drew.
    #[test]
    fn each_live_hud_layer_keeps_the_hidden_pass_running() {
        assert!(!crate::PageSnapshot::default().has_live_hud_content());
        let indicator = crate::PageSnapshot {
            stealth_indicator: Some(crate::StealthIndicatorView {
                state: byroredux_core::stealth::IndicatorState::Hidden,
                look: crate::StealthIndicatorLook::Eye,
            }),
            ..Default::default()
        };
        assert!(indicator.has_live_hud_content());
        let crosshair = crate::PageSnapshot {
            show_crosshair: true,
            ..Default::default()
        };
        assert!(crosshair.has_live_hud_content());
        // A toggle with nothing to draw stays cheap.
        let empty_vitals = crate::PageSnapshot {
            show_vitals: true,
            vitals: Some(Vec::new()),
            show_objectives: true,
            ..Default::default()
        };
        assert!(!empty_vitals.has_live_hud_content());
    }
}
