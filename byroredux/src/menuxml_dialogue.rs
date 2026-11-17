//! M48 Phase 4 GUI-PAGE-004 (#6666) — Fallout 3 / New Vegas conversations on the game's own
//! dialogue document (S2), `menus\dialog\dialog_menu.xml`.
//!
//! PRD Q1: a menu runs on the game's own document once what it needs is served, the dialogue menu
//! first; the native page stays the fallback (S3). The document's own traits say what native code
//! sets:
//!
//! - **The menu's io traits** (`io()`, authored on the root with their defaults): `_DialogVisible`
//!   (the whole menu), `_ShowingText` (the speaker's line is shown; off, the topic list is),
//!   `_ShowSubtitles`, `_MinListHeight`. The driver sets `_DialogVisible`, and `_ShowingText` from
//!   the line's speaking cue, so the list appears when the line has been presented.
//! - **The labels**: `DM_SpeakerNameLabel` (id 1) and `DM_SpeakerText` (id 2) take their `string`.
//! - **The topic list** (`DM_TopicList`, id 3, a `list_box.xml`): one `DM_TopicTemplate` row per
//!   topic, which the engine instantiates; per row `listindex` (≥ 0 shows it), `_y`, `height`
//!   (native computes it from the text; the driver uses [`ROW_HEIGHT`]), `string`, and
//!   `_line_alpha` (the row text's alpha — a topic already said is drawn at [`SAID_ALPHA`], the
//!   said-marker the native page carries). A fixed pool of [`ROW_POOL`] rows is instantiated once;
//!   rows past the topic count are hidden (`listindex` −1).
//!
//! Input reaches it through `menu_xml_input` while it owns input: a click on `DM_ClickRect` (the
//! whole screen while the line shows) or on the line skips it; a click on a row selects that
//! topic through `select_topic_by_form_id`, as the native page's click does.

use byroredux_debug_ui::DialogueTopicSnapshot;
use byroredux_menuxml::{MenuRenderer, ScreenTraits, TileHit};

use crate::hud::HudAssets;

/// The game-facing name of the dialogue menu on Fallout 3 / New Vegas (`MenuMode` 1009).
pub(crate) const DIALOGUE_MENU_CODE: u32 = 1009;
/// Topic rows instantiated once; a conversation offering more shows the first this many.
pub(crate) const ROW_POOL: usize = 16;
/// A row's height in UI pixels (native code measures it from the text, which the engine does not).
pub(crate) const ROW_HEIGHT: f32 = 34.0;
/// The `_line_alpha` of a topic the NPC has already said (an unsaid one is 255).
pub(crate) const SAID_ALPHA: f32 = 128.0;

fn row_name(index: usize) -> String {
    format!("DM_Topic{index}")
}

/// What the document asked of the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentAction {
    /// The player clicked the line: skip it.
    Skip,
    /// The player clicked topic row `index`.
    SelectTopic(usize),
}

/// The action an activated tile asks for.
pub(crate) fn action_of(hit: &TileHit) -> Option<DocumentAction> {
    let name = hit.name.as_deref()?;
    if name.eq_ignore_ascii_case("DM_ClickRect") || name.eq_ignore_ascii_case("DM_SpeakerText") {
        return Some(DocumentAction::Skip);
    }
    name.strip_prefix("DM_Topic").and_then(|index| index.parse().ok()).map(DocumentAction::SelectTopic)
}

/// The open dialogue document.
pub(crate) struct DialogueDocument {
    /// The registry's name for the menu (`DialogMenu`).
    pub(crate) name: String,
    /// The document path (`menus\dialog\dialog_menu.xml`).
    pub(crate) document: String,
    pub(crate) renderer: MenuRenderer,
    assets: HudAssets,
    texture: u32,
    width: u32,
    height: u32,
    upload_buffer: Vec<u8>,
    signature: Option<u64>,
    dirty: bool,
}

impl DialogueDocument {
    /// Load `document` through the session's `MenuProfile` and instantiate the topic rows.
    pub(crate) fn open(
        ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
        assets: HudAssets,
        name: String,
        document: String,
    ) -> Result<Self, String> {
        let (w, h) = ctx.swapchain_extent();
        let profile = assets.profile.menu;
        let mut renderer = MenuRenderer::load_with_profile(&assets, &document, ScreenTraits::new(w as f32, h as f32), profile)
            .map_err(|error| format!("{document}: {error}"))?;
        for index in 0..ROW_POOL {
            renderer
                .instantiate_template("DM_TopicTemplate", "DM_TopicList", &row_name(index))
                .map_err(|error| format!("{document}: topic row: {error}"))?;
            renderer.set_override(&row_name(index), "listindex", -1.0);
        }
        renderer.set_override("DialogMenu", "_DialogVisible", 2.0);
        let texture = {
            let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
            ctx.texture_registry
                .register_rgba(upload_ctx, w, h, &vec![0u8; (w * h * 4) as usize])
                .map_err(|error| format!("{document}: texture registration failed: {error:#}"))?
        };
        Ok(Self { name, document, renderer, assets, texture, width: w, height: h, upload_buffer: Vec::new(), signature: None, dirty: true })
    }

    /// Present `snapshot` when it changed.
    pub(crate) fn feed(&mut self, snapshot: &DialogueTopicSnapshot) {
        let signature = signature(snapshot);
        if self.signature == Some(signature) {
            return;
        }
        self.signature = Some(signature);
        self.dirty = true;
        let r = &mut self.renderer;
        r.set_override("DialogMenu", "_ShowingText", if snapshot.speaking { 2.0 } else { 0.0 });
        r.set_override_str("DM_SpeakerNameLabel", "string", &snapshot.speaker_name);
        r.set_override_str("DM_SpeakerText", "string", &snapshot.response_text);
        for index in 0..ROW_POOL {
            let row = row_name(index);
            match snapshot.topics.get(index) {
                Some(entry) => {
                    r.set_override(&row, "listindex", index as f32);
                    r.set_override(&row, "_y", index as f32 * ROW_HEIGHT);
                    r.set_override(&row, "height", ROW_HEIGHT);
                    r.set_override_str(&row, "string", &entry.name);
                    r.set_override(&row, "_line_alpha", if entry.said { SAID_ALPHA } else { 255.0 });
                }
                None => r.set_override(&row, "listindex", -1.0),
            }
        }
    }

    /// Mark the document for a fresh raster (input changed what it shows).
    pub(crate) fn touch(&mut self) {
        self.dirty = true;
    }

    /// Rasterize and upload when something changed; the texture to composite either way.
    pub(crate) fn render_and_upload(&mut self, ctx: &mut byroredux_renderer::vulkan::context::VulkanContext) -> u32 {
        if self.dirty {
            self.dirty = false;
            let pixels = self.renderer.render_frame(&self.assets);
            self.upload_buffer.clear();
            self.upload_buffer.extend_from_slice(pixels);
            let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
            if let Err(error) = ctx.texture_registry.write_rgba_inplace(upload_ctx, self.texture, self.width, self.height, &self.upload_buffer) {
                log::error!("{}: texture upload failed: {error:#}", self.document);
            }
        }
        self.texture
    }

    /// Release the texture.
    pub(crate) fn close(self, ctx: &mut byroredux_renderer::vulkan::context::VulkanContext) {
        ctx.texture_registry.drop_texture(&ctx.device, self.texture);
    }

    /// The UI pixel extent input is mapped to.
    pub(crate) fn extent(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Composite the open `document` over the UI overlay's `textures`, unless a video or loading
/// screen `covered` the frame.
pub(crate) fn composite(
    document: Option<&mut DialogueDocument>,
    ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
    covered: bool,
    textures: &mut Vec<u32>,
) {
    if let (Some(document), false) = (document, covered) {
        textures.push(document.render_and_upload(ctx));
    }
}

fn signature(snapshot: &DialogueTopicSnapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    snapshot.speaker_name.hash(&mut hasher);
    snapshot.response_text.hash(&mut hasher);
    snapshot.speaking.hash(&mut hasher);
    for entry in &snapshot.topics {
        entry.topic_form_id.hash(&mut hasher);
        entry.name.hash(&mut hasher);
        entry.said.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activations_map_to_skip_and_topic_rows() {
        let hit = |name: &str| TileHit { tile: 0, name: Some(name.to_owned()), id: None };
        assert_eq!(action_of(&hit("DM_ClickRect")), Some(DocumentAction::Skip));
        assert_eq!(action_of(&hit("DM_SpeakerText")), Some(DocumentAction::Skip));
        assert_eq!(action_of(&hit("DM_Topic3")), Some(DocumentAction::SelectTopic(3)));
        assert_eq!(action_of(&hit("scrollbar_vert_up")), None);
        assert_eq!(action_of(&TileHit { tile: 0, name: None, id: Some(4) }), None);
    }

    /// The Phase 0 design's corpus check, on the live document: the conversation's speaker, line
    /// and topics land on the tiles native code feeds, a said topic is dimmed, and a click on a
    /// row is that row's activation; with the line showing the list is hidden and a click skips.
    #[test]
    #[ignore = "needs FNV game data on disk"]
    fn fnv_dialogue_document_presents_a_conversation() {
        use byroredux_debug_ui::DialogueTopicEntryView;
        use byroredux_plugin::esm::test_paths::{data_dir, FNV_DEFAULT, FNV_ENV};
        let profile = crate::hud::HudGameProfile::fallout_nv();
        let assets = HudAssets::open_in(profile, &data_dir(FNV_ENV, FNV_DEFAULT)).expect("FNV menu archives");
        let mut r = MenuRenderer::load_with_profile(&assets, "menus\\dialog\\dialog_menu.xml", ScreenTraits::new(1280.0, 720.0), profile.menu)
            .expect("dialog_menu.xml loads");
        for index in 0..ROW_POOL {
            r.instantiate_template("DM_TopicTemplate", "DM_TopicList", &row_name(index)).expect("a row");
            r.set_override(&row_name(index), "listindex", -1.0);
        }
        r.set_override("DialogMenu", "_DialogVisible", 2.0);
        let mut doc = DialogueDocument {
            name: "DialogMenu".into(),
            document: "menus\\dialog\\dialog_menu.xml".into(),
            renderer: r,
            assets,
            texture: 0,
            width: 1280,
            height: 720,
            upload_buffer: Vec::new(),
            signature: None,
            dirty: true,
        };
        let mut snapshot = DialogueTopicSnapshot {
            npc: 1,
            speaker_name: "Doc Mitchell".into(),
            topic_form_id: 1,
            topic_name: "GREETING".into(),
            info_form_id: 2,
            response_text: "Well, you're up.".into(),
            topics: vec![
                DialogueTopicEntryView { topic_form_id: 3, name: "Who are you?".into(), said: true },
                DialogueTopicEntryView { topic_form_id: 4, name: "Goodbye.".into(), said: false },
            ],
            speaking: true,
            voiced: true,
            choices: false,
        };
        doc.feed(&snapshot);
        let names = |doc: &DialogueDocument| doc.renderer.hit_targets().iter().filter_map(|t| doc.renderer.hit_test(t.rect.x + 1.0, t.rect.y + 1.0)).filter_map(|h| h.name).collect::<Vec<_>>();
        assert!(names(&doc).iter().any(|n| n == "DM_ClickRect"), "the line shows: the click rect takes the pointer");
        doc.renderer.pointer_down(640.0, 100.0);
        doc.renderer.pointer_up(640.0, 100.0);
        assert_eq!(doc.renderer.take_activations().iter().filter_map(action_of).collect::<Vec<_>>(), [DocumentAction::Skip]);

        snapshot.speaking = false;
        doc.feed(&snapshot);
        let targets = doc.renderer.hit_targets();
        let row = |index: usize| {
            let tile = doc.renderer.tile(&row_name(index)).unwrap();
            *targets.iter().find(|t| t.tile == tile).unwrap_or_else(|| panic!("row {index} is a target: {targets:?}"))
        };
        let second = row(1);
        let (x, y) = (second.rect.x + second.rect.w / 2.0, second.rect.y + second.rect.h / 2.0);
        assert_eq!(doc.renderer.hit_test(x, y).and_then(|h| h.name).as_deref(), Some("DM_Topic1"));
        doc.renderer.pointer_down(x, y);
        doc.renderer.pointer_up(x, y);
        assert_eq!(doc.renderer.take_activations().iter().filter_map(action_of).collect::<Vec<_>>(), [DocumentAction::SelectTopic(1)]);
        assert!(!targets.iter().any(|t| Some(t.tile) == doc.renderer.tile(&row_name(2))), "unused rows are hidden");
        let pixels = doc.renderer.render_frame(&doc.assets);
        assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0), "the document draws");
    }
}
