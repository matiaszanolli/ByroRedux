//! Acquire and lookup: resolving an authored texture path to a live
//! `TextureHandle`, the per-entry metadata the `GpuInstance` build loop
//! reads, and the two fallbacks a miss resolves to.
//!
//! `get_*` borrows without touching the refcount; `acquire_*` takes a
//! reference the caller must pair with a `drop_texture` (#524).

use super::*;

impl TextureRegistry {
    /// Whether the texture at `handle` was loaded from a DDS format that
    /// carries an alpha channel (BC2/BC3/BC7/RGBA). False for alpha-less
    /// normals (BC5/BC4/BC1) and handles never loaded from DDS (fallbacks).
    /// Gates the normal-alpha-as-spec gloss path (Skyrim/Gamebryo author the
    /// gloss mask in the normal alpha; BC5 normals have none).
    ///
    /// #3682 — direct `Vec` index into the dense-handle-keyed registry
    /// instead of a `HashMap` probe; this is read once per draw command in
    /// the `GpuInstance` build loop, the busiest read site in the cluster
    /// #3061 already converted the rest of.
    pub fn handle_has_alpha(&self, handle: TextureHandle) -> bool {
        self.textures
            .get(handle as usize)
            .is_some_and(|entry| entry.has_alpha)
    }

    /// Average texel colour of the diffuse texture at `handle`, in raw
    /// monitor (sRGB-encoded) space — `None` for fallback handles, normal/
    /// mask maps, BC7, and any handle never loaded from a diffuse-colour
    /// DDS. Folded into the GI bounce albedo (#1628) so textured surfaces
    /// bleed their mean texel colour, not just the flat material tint.
    /// Computed once at upload; this is a cheap cached lookup.
    ///
    /// #3682 — same `Vec`-index move as `handle_has_alpha`; this is the
    /// unconditional (every draw command, not just alpha-blend ones)
    /// sibling read in the same loop.
    pub fn handle_avg_rgb(&self, handle: TextureHandle) -> Option<[f32; 3]> {
        self.textures.get(handle as usize).and_then(|e| e.avg_rgb)
    }

    /// Look up a cached texture by path. Returns `None` if not loaded.
    ///
    /// Read-only probe — does **not** bump the entry's refcount. Use
    /// [`acquire_by_path`](Self::acquire_by_path) when the caller
    /// intends to hold the handle and must pair it with a
    /// `drop_texture`. See #524.
    pub fn get_by_path(&self, path: &str) -> Option<TextureHandle> {
        // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT cache entry.
        self.get_by_path_with_clamp(path, 3)
    }

    /// `get_by_path`'s clamp-aware variant — looks up the cache entry
    /// for `(path, clamp_mode)`. Pre-#610 the cache was keyed by path
    /// alone; today the same path with two different clamp modes
    /// produces two distinct entries so the descriptor write picks the
    /// right `VkSamplerAddressMode`. The legacy single-key shape is the
    /// `clamp_mode == 3` (`WRAP_S_WRAP_T` = REPEAT/REPEAT) entry — the
    /// mode `get_by_path` passes and the one nif.xml defaults to; `0` is
    /// `CLAMP_S_CLAMP_T` (#3757).
    pub fn get_by_path_with_clamp(&self, path: &str, clamp_mode: u8) -> Option<TextureHandle> {
        let clamp_mode = clamp_mode.min(3);
        self.path_map
            .get(&texture_keyed_path(path, clamp_mode, TextureViewKind::D2))
            .copied()
    }

    /// Acquire a texture handle by path, bumping the refcount on hit.
    ///
    /// Mirror of [`get_by_path`](Self::get_by_path) but with the
    /// refcount side-effect. The caller must pair this with a single
    /// `drop_texture` when the handle is no longer in use. `resolve_texture`
    /// uses this on its fast path (before falling through to
    /// `load_dds` on miss) so every cell-loader resolve ends up with
    /// exactly one acquire. See #524.
    pub fn acquire_by_path(&mut self, path: &str) -> Option<TextureHandle> {
        // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT cache entry.
        self.acquire_by_path_with_clamp(path, 3)
    }

    /// `acquire_by_path`'s clamp-aware variant. Same refcount semantics
    /// as the legacy entry point; the cache lookup includes
    /// `clamp_mode` so a clamping request resolves to its own entry
    /// instead of accidentally adopting the REPEAT-bound descriptor —
    /// which is the `clamp_mode == 3` entry, not `0` (#3757). See
    /// #610 / D4-NEW-02.
    pub fn acquire_by_path_with_clamp(
        &mut self,
        path: &str,
        clamp_mode: u8,
    ) -> Option<TextureHandle> {
        self.acquire_by_path_with_clamp_and_color_space(path, clamp_mode, TextureColorSpace::Srgb)
    }

    /// Role-aware cache acquisition. Linear and sRGB views of the same path
    /// cannot alias because Vulkan bakes the transfer function into the image
    /// format used by the descriptor.
    pub fn acquire_by_path_with_clamp_and_color_space(
        &mut self,
        path: &str,
        clamp_mode: u8,
        color_space: TextureColorSpace,
    ) -> Option<TextureHandle> {
        self.acquire_by_path_for_view(path, clamp_mode, TextureViewKind::D2, color_space)
    }

    /// Cubemap counterpart of [`Self::acquire_by_path_with_clamp`]. Cube and
    /// 2D requests for the same DDS path deliberately occupy distinct cache
    /// keys because they target different descriptor bindings.
    pub fn acquire_cubemap_by_path_with_clamp(
        &mut self,
        path: &str,
        clamp_mode: u8,
    ) -> Option<TextureHandle> {
        self.acquire_by_path_for_view(
            path,
            clamp_mode,
            TextureViewKind::Cube,
            TextureColorSpace::Srgb,
        )
    }

    fn acquire_by_path_for_view(
        &mut self,
        path: &str,
        clamp_mode: u8,
        view_kind: TextureViewKind,
        color_space: TextureColorSpace,
    ) -> Option<TextureHandle> {
        let clamp_mode = clamp_mode.min(3);
        let normalized =
            texture_keyed_path_with_color_space(path, clamp_mode, view_kind, color_space);
        let &handle = self.path_map.get(&normalized)?;
        let entry = self.textures.get_mut(handle as usize)?;
        entry.ref_count = entry.ref_count.saturating_add(1);
        Some(handle)
    }

    /// Handle for the fallback checkerboard texture (always 0). Indicates
    /// "this entity was supposed to have a real texture, but lookup failed."
    pub fn fallback(&self) -> TextureHandle {
        self.fallback_handle
    }

    /// Handle for the neutral (white 1×1) fallback (always 1). Indicates
    /// "this entity intentionally has no diffuse texture authored — the
    /// material's emissive / alpha / vertex-color terms drive the surface
    /// color." See [`Self::set_neutral_fallback`].
    pub fn neutral_fallback(&self) -> TextureHandle {
        self.neutral_fallback_handle
    }
}
