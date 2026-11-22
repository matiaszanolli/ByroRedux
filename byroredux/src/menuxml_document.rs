//! M48 Phase 4 (#6666) — a Fallout 3 / New Vegas menu open on the game's own MenuXml document
//! (S2): the document loaded through the session's `MenuProfile`, rasterized when what it shows
//! changed, and composited over the HUD as a UI texture. Input reaches it through
//! `menu_xml_input` while the registry names it the input owner; its activations stay in the
//! renderer until its driver takes them.
//!
//! What a document shows is its driver's: each menu the engine runs on a document stands in for
//! the native code that fed it, setting the traits the document marks "set in code"
//! (`menuxml_dialogue` for the dialogue menu, `menuxml_message` for script message boxes).

use byroredux_menuxml::{MenuRenderer, ScreenTraits};

use crate::hud::HudAssets;

/// One menu open on its document.
pub(crate) struct MenuDocument {
    /// The menu's `MenuMode` code (1009 `DialogMenu`, 1001 `MessageMenu` …).
    pub(crate) code: u32,
    /// The registry's name for the menu.
    pub(crate) name: String,
    /// The document path (`menus\dialog\dialog_menu.xml`).
    pub(crate) document: String,
    pub(crate) renderer: MenuRenderer,
    /// The driver's fingerprint of what it last fed, so an unchanged state is not re-sent.
    pub(crate) signature: Option<u64>,
    assets: HudAssets,
    texture: u32,
    width: u32,
    height: u32,
    upload_buffer: Vec<u8>,
    dirty: bool,
}

impl MenuDocument {
    /// Load `document` through the session's `MenuProfile` and register its UI texture.
    pub(crate) fn open(
        ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
        assets: HudAssets,
        code: u32,
        name: String,
        document: String,
    ) -> Result<Self, String> {
        let (w, h) = ctx.swapchain_extent();
        let renderer = MenuRenderer::load_with_profile(&assets, &document, ScreenTraits::new(w as f32, h as f32), assets.profile.menu)
            .map_err(|error| format!("{document}: {error}"))?;
        let texture = {
            let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
            ctx.texture_registry
                .register_rgba(upload_ctx, w, h, &vec![0u8; (w * h * 4) as usize])
                .map_err(|error| format!("{document}: texture registration failed: {error:#}"))?
        };
        Ok(Self::with(code, name, document, renderer, assets, texture, (w, h)))
    }

    fn with(code: u32, name: String, document: String, renderer: MenuRenderer, assets: HudAssets, texture: u32, (width, height): (u32, u32)) -> Self {
        Self { code, name, document, renderer, signature: None, assets, texture, width, height, upload_buffer: Vec::new(), dirty: true }
    }

    /// A document with no GPU texture, for the corpus tests.
    #[cfg(test)]
    pub(crate) fn headless(code: u32, name: &str, document: &str, assets: HudAssets, size: (u32, u32)) -> Self {
        let renderer = MenuRenderer::load_with_profile(&assets, document, ScreenTraits::new(size.0 as f32, size.1 as f32), assets.profile.menu)
            .unwrap_or_else(|error| panic!("{document} loads: {error}"));
        Self::with(code, name.to_owned(), document.to_owned(), renderer, assets, 0, size)
    }

    /// Mark the document for a fresh raster (its driver or its input changed what it shows).
    pub(crate) fn touch(&mut self) {
        self.dirty = true;
    }

    /// Rasterize the document as it stands (the session's menu assets), for the corpus tests.
    #[cfg(test)]
    pub(crate) fn render(&mut self) -> &[u8] {
        self.renderer.render_frame(&self.assets)
    }

    /// Rasterize and upload when something changed; the texture to composite either way.
    fn render_and_upload(&mut self, ctx: &mut byroredux_renderer::vulkan::context::VulkanContext) -> u32 {
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

/// Composite the open `documents` (in opening order, the last on top) over the UI overlay's
/// `textures`, unless a video or loading screen `covered` the frame.
pub(crate) fn composite(
    documents: &mut [MenuDocument],
    ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
    covered: bool,
    textures: &mut Vec<u32>,
) {
    if covered {
        return;
    }
    for document in documents {
        textures.push(document.render_and_upload(ctx));
    }
}
