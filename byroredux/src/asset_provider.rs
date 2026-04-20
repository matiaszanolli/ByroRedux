//! BSA/BA2-backed texture and mesh extraction.

use byroredux_renderer::VulkanContext;

/// A game archive that can extract files by path.
/// Wraps either a BSA (Oblivion–Skyrim SE) or BA2 (FO4–Starfield) archive.
enum Archive {
    Bsa(byroredux_bsa::BsaArchive),
    Ba2(byroredux_bsa::Ba2Archive),
}

impl Archive {
    /// Open an archive file, auto-detecting BSA vs BA2 from the file magic.
    fn open(path: &str) -> Result<Self, String> {
        let magic = std::fs::read(path)
            .map_err(|e| format!("read '{}': {}", path, e))
            .and_then(|data| {
                if data.len() < 4 {
                    Err(format!("'{}' too small", path))
                } else {
                    Ok([data[0], data[1], data[2], data[3]])
                }
            })?;
        if &magic == b"BTDX" {
            byroredux_bsa::Ba2Archive::open(path)
                .map(Archive::Ba2)
                .map_err(|e| format!("BA2 '{}': {}", path, e))
        } else {
            byroredux_bsa::BsaArchive::open(path)
                .map(Archive::Bsa)
                .map_err(|e| format!("BSA '{}': {}", path, e))
        }
    }

    fn extract(&self, path: &str) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Archive::Bsa(a) => a.extract(path),
            Archive::Ba2(a) => a.extract(path),
        }
    }
}

/// Provides file data by searching BSA/BA2 archives.
pub(crate) struct TextureProvider {
    texture_archives: Vec<Archive>,
    mesh_archives: Vec<Archive>,
}

impl TextureProvider {
    pub(crate) fn new() -> Self {
        Self {
            texture_archives: Vec::new(),
            mesh_archives: Vec::new(),
        }
    }

    /// Extract a texture (DDS) from texture archives.
    ///
    /// Paths are normalized before lookup: anything that doesn't already
    /// start with `textures\` gets the prefix prepended. Bethesda WTHR /
    /// CLMT / LTEX records author paths relative to the `textures\`
    /// root (e.g. `sky\cloudsnoon.dds`, `landscape\dirt02.dds`) but
    /// the archive layer stores them with the full `textures\` prefix.
    /// Pre-#468 every such lookup silently missed and clouds / sun
    /// textures rendered as disabled. Callers that already supply a
    /// fully-qualified path (the cell loader's `textures\landscape\…`
    /// path-building sites) go through unchanged.
    pub(crate) fn extract(&self, path: &str) -> Option<Vec<u8>> {
        let normalized = normalize_texture_path(path);
        for archive in &self.texture_archives {
            if let Ok(data) = archive.extract(normalized.as_ref()) {
                return Some(data);
            }
        }
        None
    }

    /// Extract a mesh (NIF) from mesh archives.
    pub(crate) fn extract_mesh(&self, path: &str) -> Option<Vec<u8>> {
        for archive in &self.mesh_archives {
            if let Ok(data) = archive.extract(path) {
                return Some(data);
            }
        }
        None
    }
}

/// Prepend `textures\` to a texture path if the path doesn't already
/// begin with that segment (case-insensitive). Returns `Cow::Borrowed`
/// when the input is already fully qualified so we don't allocate on
/// the hot path (every REFR material resolves a texture).
///
/// See #468 — Bethesda WTHR cloud / CLMT sun / LTEX landscape records
/// all author paths relative to the `textures\` root, but the BSA / BA2
/// layer stores them with the full prefix.
pub(crate) fn normalize_texture_path(path: &str) -> std::borrow::Cow<'_, str> {
    // Fast ASCII-lowercase check on just the first 9 bytes (`textures\`).
    // Avoids allocating a full lowercase copy of the whole path on every
    // texture lookup. Matches on either `/` or `\` separators — archive
    // paths are backslashed on Bethesda systems, but forward slashes can
    // sneak in via mod authoring tools.
    let bytes = path.as_bytes();
    let has_prefix = bytes.len() >= 9
        && bytes[..8].eq_ignore_ascii_case(b"textures")
        && (bytes[8] == b'\\' || bytes[8] == b'/');
    if has_prefix {
        std::borrow::Cow::Borrowed(path)
    } else {
        std::borrow::Cow::Owned(format!("textures\\{}", path))
    }
}

/// Parse grid coordinates from a "x,y" string.
pub(crate) fn parse_grid_coords(s: &str) -> (i32, i32) {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        let x = parts[0].trim().parse::<i32>().unwrap_or(0);
        let y = parts[1].trim().parse::<i32>().unwrap_or(0);
        (x, y)
    } else {
        log::warn!("Invalid grid format '{}', using (0,0)", s);
        (0, 0)
    }
}

/// Build a TextureProvider from CLI arguments.
pub(crate) fn build_texture_provider(args: &[String]) -> TextureProvider {
    let mut provider = TextureProvider::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--textures-bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => {
                            log::info!("Opened textures archive: '{}'", path);
                            provider.texture_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open textures archive: {}", e),
                    }
                    i += 2;
                    continue;
                }
            }
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => {
                            log::info!("Opened mesh archive: '{}'", path);
                            provider.mesh_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open mesh archive: {}", e),
                    }
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    provider
}

/// Resolve a texture path to a texture handle, with BSA/BA2 lookup and caching.
pub(crate) fn resolve_texture(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
) -> u32 {
    let Some(tex_path) = tex_path else {
        return ctx.texture_registry.fallback();
    };
    if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
        return cached;
    }
    if let Some(dds_bytes) = tex_provider.extract(tex_path) {
        let alloc = ctx.allocator.as_ref().unwrap();
        match ctx.texture_registry.load_dds(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.transfer_pool,
            tex_path,
            &dds_bytes,
        ) {
            Ok(h) => {
                log::info!("Loaded DDS texture: '{}'", tex_path);
                return h;
            }
            Err(e) => {
                log::warn!("Failed to load DDS '{}': {}", tex_path, e);
            }
        }
    } else {
        log::debug!("Texture not found in archive: '{}'", tex_path);
    }
    ctx.texture_registry.fallback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_prefix_when_missing() {
        // WTHR cloud path authored relative to `textures\` root.
        let out = normalize_texture_path("sky\\cloudsnoon.dds");
        assert_eq!(out.as_ref(), "textures\\sky\\cloudsnoon.dds");
    }

    #[test]
    fn normalize_leaves_fully_qualified_paths_borrowed() {
        // Cell loader's landscape path path-building already supplies
        // the `textures\` prefix; the fn must pass through without
        // allocating (Cow::Borrowed).
        let input = "textures\\landscape\\dirt02.dds";
        let out = normalize_texture_path(input);
        assert_eq!(out.as_ref(), input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_is_case_insensitive_on_prefix() {
        // A future tool or mod authoring flow might export
        // `Textures\…` or `TEXTURES\…`; the prefix check is ASCII-
        // case-insensitive and shouldn't double up.
        let out = normalize_texture_path("Textures\\sky\\cloudsnoon.dds");
        assert_eq!(out.as_ref(), "Textures\\sky\\cloudsnoon.dds");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_accepts_forward_slash_separator() {
        // Mod authoring tools occasionally emit forward slashes.
        // The prefix check accepts either.
        let out = normalize_texture_path("textures/sky/cloudsnoon.dds");
        assert_eq!(out.as_ref(), "textures/sky/cloudsnoon.dds");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_prefixes_non_textures_paths_as_owned() {
        // Any path whose first segment isn't `textures\` — e.g. a
        // relative CLMT `sun_01.dds` or a broken mod export — gets
        // the prefix prepended. The fn allocates in this branch, so
        // result is Cow::Owned.
        let out = normalize_texture_path("sun_01.dds");
        assert_eq!(out.as_ref(), "textures\\sun_01.dds");
        assert!(matches!(out, std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn normalize_short_string_gets_prefixed() {
        // Guard against the `bytes.len() >= 9` check: a 1-byte path
        // should still prefix cleanly.
        let out = normalize_texture_path("a");
        assert_eq!(out.as_ref(), "textures\\a");
    }
}
