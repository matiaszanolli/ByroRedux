//! The snapshot the panels render and the actions they hand back (#6578, split from `panels.rs`).


use byroredux_core::settings::{SettingChange, SettingEntry};
use byroredux_sdk::studio::{StudioCommand, StudioSnapshot};

/// Read-only snapshot of the engine-side state the panels render.
/// Built each frame by the binary right before `DebugUiState::run`.
/// Cloning is cheap — `MetricsSnapshot` is small, the entity list
/// is name-only.
#[derive(Default, Clone)]
pub struct PanelSnapshot {
    /// Original game's LSCR tip while its loading artwork is composited.
    pub loading_tip: Option<String>,
    /// Native in-world HUD prompt. Unlike the debug panels, this remains
    /// visible when the F3 operator overlay is closed.
    ///
    /// `&'static str`, not `String` (#2680 / PERF-D1-02): this is the one
    /// field rebuilt on every frame including the overlay-hidden path, and
    /// every prompt the producer can name is a compile-time constant. A
    /// future prompt that interpolates a reference name wants
    /// `Cow<'static, str>` here, not a per-frame `String`.
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
    /// #5822 — core's snapshot directly; debug-ui depends on core, so
    /// no hand-synced view copy is needed.
    pub metrics: Option<byroredux_core::ecs::MetricsSnapshot>,
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
    /// `(entity_id, name)` pairs. `None` until the operator opens
    /// the Entities panel — populating this on every frame would
    /// be unnecessary work for an overlay that's hidden most of
    /// the time.
    pub entities: Option<Vec<(u32, String)>>,
    /// Present when the executable is hosting an SDK Studio document.
    pub studio: Option<StudioSnapshot>,
}

/// One script message box (`Message.Show` on a `MESG` with its "Message
/// Box" flag): title, formatted text, and the authored buttons.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageBoxView {
    pub title: String,
    pub text: String,
    pub buttons: Vec<String>,
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

/// Allocation-free native prompt assembled from the active binding and the
/// selected interaction's verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InteractionPrompt {
    pub binding: &'static str,
    pub verb: &'static str,
}

/// One vitals bar's presentation state: the composed current value and the
/// undamaged maximum of one canonical actor value. Labels are compile-time
/// constants resolved once per plugin load, so a frame rebuild allocates no
/// strings (same contract as `InteractionPrompt`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VitalBarView {
    pub label: &'static str,
    pub current: f32,
    pub max: f32,
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
/// visible. Plain data: `debug-ui` cannot depend on the binary's component
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

/// Actions the panels asked the App to perform. Drained by the
/// binary after [`DebugUiState::run`] returns.
#[derive(Default, Clone)]
pub struct PanelOutputs {
    /// NIF / cell loads to queue against `PendingDebugLoadSlot`.
    pub queued_loads: Vec<QueuedLoad>,
    /// Console expressions to evaluate via the existing
    /// `CommandRegistry`. The binary translates each into the same
    /// path the debug-server's `Eval` request takes.
    pub console_evals: Vec<String>,
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
    /// Renderer-independent Studio mutations for the host to apply.
    pub studio_commands: Vec<StudioCommand>,
    /// True when the operator asked to refresh the entity list.
    /// The binary rebuilds the snapshot's `entities` next frame.
    pub refresh_entities: bool,
    /// Close the native pause menu and return input to the world.
    pub resume_game: bool,
    /// Save to the next ring slot through the binary's canonical save API.
    pub quicksave: bool,
    /// Load the most recently written slot through the canonical load queue.
    pub quickload: bool,
    /// Perform the same orderly shutdown as the window close button.
    pub quit_game: bool,
}

/// One queued load request. The binary maps this 1:1 onto a
/// `PendingDebugLoad` enum variant — kept as a separate type here
/// so the debug-ui crate doesn't need to depend on core's
/// `PendingDebugLoad` directly.
#[derive(Debug, Clone)]
pub enum QueuedLoad {
    Nif { path: String, label: Option<String> },
}
