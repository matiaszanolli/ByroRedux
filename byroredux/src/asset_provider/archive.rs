/// A game archive that can extract files by path.
/// Wraps either a BSA (Oblivion–Skyrim SE) or BA2 (FO4–Starfield) archive.
pub(crate) enum Archive {
    Bsa(byroredux_bsa::BsaArchive),
    Ba2(byroredux_bsa::Ba2Archive),
}

impl Archive {
    /// Open an archive file, auto-detecting BSA vs BA2 from the file magic.
    pub(crate) fn open(path: &str) -> Result<Self, String> {
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

    pub(crate) fn extract(&self, path: &str) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Archive::Bsa(a) => a.extract(path),
            Archive::Ba2(a) => a.extract(path),
        }
    }

    /// Enumerate entry paths (BA2 paths are already lowercase +
    /// backslash-separated, per `Ba2Archive::list_files`). BSA archives
    /// return empty: Starfield's component databases ship only in BA2s,
    /// so a BSA can't carry one. Used by Starfield CDB discovery (#1571).
    pub(crate) fn list_files(&self) -> Vec<&str> {
        match self {
            Archive::Bsa(_) => Vec::new(),
            Archive::Ba2(a) => a.list_files(),
        }
    }
}

/// Prepend `meshes\` to a NIF path when the input doesn't already
/// start with that segment (case-insensitive, accepts either
/// separator). `MODL` sub-records on RACE / NPC_ / ARMO records are
/// authored relative to the `meshes\` root; the BSA layer stores the
/// full prefix. Allocation only fires when the prefix is missing —
/// already-prefixed paths borrow.
///
/// Mirror of the static-spawn path's manual prefix-prepend at
/// [`cell_loader::references`] line ~421 (which predates this
/// helper; the cell-loader form is now a redundant idempotent
/// double-normalisation and can be removed in a follow-up sweep).
pub fn normalize_mesh_path(path: &str) -> std::borrow::Cow<'_, str> {
    let bytes = path.as_bytes();
    if bytes.len() >= 7 {
        let head = &bytes[..7];
        if head.eq_ignore_ascii_case(b"meshes\\") || head.eq_ignore_ascii_case(b"meshes/") {
            return std::borrow::Cow::Borrowed(path);
        }
    }
    // #1292 — Starfield content-addressed BSGeometry external `.mesh`
    // companion files live at `geometries\<hash>.mesh` (NO `meshes\`
    // prefix). The importer at `crates/nif/src/import/mesh/bs_geometry.rs`
    // composes the canonical path before calling the resolver; without
    // this gate the normaliser silently prepended `meshes\` and turned
    // every Starfield poster / architecture / set-dressing lookup into
    // a guaranteed miss → 99.7% spawn-rate failure on Cydonia.
    if bytes.len() >= 11 {
        let head = &bytes[..11];
        if head.eq_ignore_ascii_case(b"geometries\\") || head.eq_ignore_ascii_case(b"geometries/") {
            return std::borrow::Cow::Borrowed(path);
        }
    }
    std::borrow::Cow::Owned(format!(r"meshes\{}", path))
}

/// Strip the Bethesda build-server prefix from an asset path.
///
/// Some shipping Bethesda content — most notably the Skyrim Anniversary
/// Edition's "Skyrim HD" trees, plants, and landscape clutter — embeds
/// texture and model paths with the full pipeline-internal prefix
/// `skyrimhd\build\pc\data\textures\…`. The real Bethesda engine
/// resolves these against a `Data\` root by stripping everything up to
/// and including the last `\data\` (or `/data/`) segment in the path.
/// Without that step the BSA lookup misses every affected asset and
/// the renderer falls back to the magenta-checker placeholder — the
/// symptom that prompted this fix on a Markarth grid (juniper, reach
/// branches, driftwood, plus a long tail of landscape clutter).
///
/// Returns `Cow::Borrowed` on the common case (no embedded `\data\`).
/// Case-insensitive on the `data` token; matches `\` or `/` separators
/// on either side (mod-authoring tools sometimes export forward
/// slashes).
pub(crate) fn strip_build_prefix(path: &str) -> std::borrow::Cow<'_, str> {
    let bytes = path.as_bytes();
    // We need at least `\data\X` (7 bytes) to even have a useful strip,
    // and that the strip leaves a non-empty trailer.
    if bytes.len() < 7 {
        return std::borrow::Cow::Borrowed(path);
    }
    // Scan left-to-right for the LAST `\data\` boundary so we tolerate
    // future build-server prefixes that nest a `data\` directory
    // elsewhere in the path. Pre-#945 fix used a hardcoded
    // `skyrimhd\build\pc\data\` strip but that's brittle: AE post-launch
    // patches and Creation Club mods author new prefixes
    // (`fishingrod\data\`, `survivalmode\data\`, etc.) and the engine
    // strips all of them.
    let mut last: Option<usize> = None;
    let mut i = 0;
    while i + 6 <= bytes.len() {
        let l = bytes[i];
        let r = bytes[i + 5];
        if (l == b'\\' || l == b'/')
            && (r == b'\\' || r == b'/')
            && bytes[i + 1..i + 5].eq_ignore_ascii_case(b"data")
        {
            last = Some(i + 6);
        }
        i += 1;
    }
    match last {
        Some(start) if start < bytes.len() => std::borrow::Cow::Owned(path[start..].to_string()),
        _ => std::borrow::Cow::Borrowed(path),
    }
}

/// Normalize a BGSM/BGEM material path into the archive's canonical
/// `materials\…` backslashed form. Four transformations applied in
/// order:
///
/// 1. **Build-pipeline prefix strip**: drop everything up to and
///    including the last `\data\` (or `/data/`) segment. Covers
///    `c:\projects\fallout4\build\pc\data\materials\…` — the form
///    Bethesda authors into vanilla FO4 BGSM file paths (live
///    observation on MedTek Research: 11/12 unique missing-material
///    entries used this form).
/// 2. **Leading `data\` strip**: when the path begins with `data\`
///    or `data/` (no preceding separator), trim that off. Some
///    BGSM template parents author this form (observed:
///    `data\materials\setdressing\metaltrashcan01alpha.bgsm`).
///    `strip_build_prefix` doesn't catch this case because it
///    requires a separator BEFORE `data`.
/// 3. **Forward-slash → backslash**: BA2 archives index with
///    backslashes; some BGSM `root_material_path` fields author
///    with forward slashes (observed: `template/defaulttemplate_wet.bgsm`).
///    Mod-authoring tools and DLC content also mix the two.
/// 4. **Prepend `materials\`**: when the path doesn't already start
///    with `materials\` (after the above strips), add it. BGSM
///    template parents author relative-to-materials-root paths
///    like `template/defaulttemplate_wet.bgsm`; the BA2 index has
///    them at `materials\template\…`.
///
/// Returns `Cow::Borrowed` only on the already-canonical case
/// (starts with `materials\`, no slashes/data/build-prefix). Every
/// authored-non-canonical path returns a single owned allocation.
///
/// See #FO4-D6-NEW (this issue body) for the live `tex.missing`
/// evidence that motivated each of the four transformations.
pub(crate) fn normalize_material_path(path: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    // Step 1 — build-pipeline strip.
    let after_build = strip_build_prefix(path);

    // Step 2 — leading `data\` / `data/` strip (case-insensitive).
    let after_data: Cow<'_, str> = {
        let bytes = after_build.as_bytes();
        if bytes.len() >= 5
            && bytes[..4].eq_ignore_ascii_case(b"data")
            && (bytes[4] == b'\\' || bytes[4] == b'/')
        {
            // Borrow the trailer from `after_build`. If `after_build`
            // is borrowed, the new slice stays borrowed; if it was
            // already owned, we allocate (rare).
            match after_build {
                Cow::Borrowed(s) => Cow::Borrowed(&s[5..]),
                Cow::Owned(s) => Cow::Owned(s[5..].to_string()),
            }
        } else {
            after_build
        }
    };

    // Step 3 — forward-slash → backslash. Only allocate when at
    // least one `/` is present.
    let after_sep: Cow<'_, str> = if after_data.contains('/') {
        Cow::Owned(after_data.replace('/', "\\"))
    } else {
        after_data
    };

    // Step 4 — prepend `materials\` if missing. Case-insensitive
    // on the prefix check so `Materials\foo.bgsm` doesn't get
    // double-prefixed.
    let bytes = after_sep.as_bytes();
    let has_materials =
        bytes.len() >= 10 && bytes[..9].eq_ignore_ascii_case(b"materials") && bytes[9] == b'\\';
    if has_materials {
        after_sep
    } else {
        Cow::Owned(format!("materials\\{}", after_sep))
    }
}

/// Normalize a texture path into the archive's canonical
/// `textures\…` backslashed form. Mirrors the BGSM-side
/// [`normalize_material_path`] but is texture-specific.
///
/// Two transformations applied in order:
///
/// 1. **Leading `data\` strip** (case-insensitive): when the path
///    begins with `data\` or `data/`, trim that off. FO4 head NIFs'
///    `BSShaderTextureSet` authors per-NPC FaceGen textures with
///    this form (live observation 2026-05-26 on InstituteBioScience:
///    9 / 10 unique missing-texture entries were
///    `data\textures\actors\character\facecustomization\…\<formid>_d.dds`;
///    the archive stores them at `textures\…` without the `data\`
///    prefix). `strip_build_prefix` does not catch this case because
///    it requires a separator BEFORE `data`.
/// 2. **Prepend `textures\`**: when the path doesn't already start
///    with `textures\` (after the strip above), add it. Bethesda
///    WTHR cloud / CLMT sun / LTEX landscape records all author
///    paths relative to the `textures\` root.
///
/// Returns `Cow::Borrowed` only on the canonical case (starts with
/// `textures\`, no leading `data\` prefix). Every authored-non-
/// canonical path returns a single owned allocation.
///
/// See #468 (the original `textures\` prefix issue) and F1.1 from
/// the 2026-05-26 Fallout symptom sweep (the FaceGen leading-data
/// case).
pub(crate) fn normalize_texture_path(path: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    // Step 1 — leading `data\` strip. Check the first 5 bytes:
    // `data` + separator. Borrow the trailer to keep allocations on
    // the rare path.
    let bytes = path.as_bytes();
    let after_data: Cow<'_, str> = if bytes.len() >= 5
        && bytes[..4].eq_ignore_ascii_case(b"data")
        && (bytes[4] == b'\\' || bytes[4] == b'/')
    {
        Cow::Borrowed(&path[5..])
    } else {
        Cow::Borrowed(path)
    };

    // Step 2 — prepend `textures\` if missing. Case-insensitive on
    // the first 8 bytes; matches `/` or `\` as the separator after.
    let bytes = after_data.as_bytes();
    let has_prefix = bytes.len() >= 9
        && bytes[..8].eq_ignore_ascii_case(b"textures")
        && (bytes[8] == b'\\' || bytes[8] == b'/');
    if has_prefix {
        after_data
    } else {
        Cow::Owned(format!("textures\\{}", after_data))
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

/// Open the requested archive plus any numeric-suffixed siblings.
///
/// FNV ships its base textures across `Fallout - Textures.bsa` and
/// `Fallout - Textures2.bsa`; passing only the first leaves Doc
/// Mitchell's plaster and floor textures resolving to the
/// missing-texture checkerboard placeholder, which compositing with
/// the (correctly loaded) tangent-space normal map produced the
/// "chrome posterized walls" diagnosis chased through R1 / #783 /
/// #784. By auto-loading `<stem>2.bsa` … `<stem>9.bsa` siblings when
/// the explicitly named archive ends in an unsuffixed `.bsa`, FNV's
/// split is transparent.
///
/// Skyrim splits its assets across a **zero-based** numbered series
/// instead — `Skyrim - Textures0.bsa` … `Textures8.bsa`,
/// `Skyrim - Meshes0.bsa` / `Meshes1.bsa`. The distant-LOD pipeline made
/// this load-bearing: the object-LOD atlas (`<world>.objects.dds`) and the
/// per-quad `.btr` terrain diffuse live in `Textures7.bsa`, and the `.btr` /
/// `.bto` meshes in `Meshes1.bsa` — none of which the user passes when they
/// name only the `…0` archive, so distant LOD rendered untextured (M35).
/// So when the named archive ends in `…0` (and the char before the `0` is
/// not itself a digit — i.e. it is the series START, not `…10`), auto-load
/// `…1.bsa` … `…9.bsa`. A non-zero trailing digit (`…2.bsa`, `…3.bsa`) still
/// auto-loads nothing — that path is a user listing each member explicitly,
/// or a mid-series archive we must not re-expand.
///
/// All cases are harmless when a sibling simply doesn't exist (skipped).
pub(crate) fn open_with_numeric_siblings(path: &str, kind: &str, archives: &mut Vec<Archive>) {
    match Archive::open(path) {
        Ok(a) => {
            log::info!("Opened {} archive: '{}'", kind, path);
            archives.push(a);
        }
        Err(e) => {
            log::warn!("Failed to open {} archive: {}", kind, e);
            return;
        }
    }
    for sibling in numeric_sibling_paths(path) {
        if !std::path::Path::new(&sibling).is_file() {
            continue;
        }
        match Archive::open(&sibling) {
            Ok(a) => {
                log::info!("Opened sibling {} archive: '{}'", kind, sibling);
                archives.push(a);
            }
            Err(e) => {
                log::warn!("Failed to open sibling {} archive: {}", kind, e);
            }
        }
    }
}

/// Candidate numeric-sibling archive paths for an explicitly-named archive
/// (the primary `path` itself is excluded). Pure (no I/O) so the case logic —
/// the risky part — is unit-testable; the caller filters to existing files.
///
///   * `Foo.bsa`  (no trailing digit, FNV) → `Foo2.bsa` … `Foo9.bsa`
///   * `Foo0.bsa` (zero-based series start, Skyrim) → `Foo1.bsa` … `Foo9.bsa`
///   * `Foo2.bsa` (mid-series digit) → none (the user lists members explicitly)
///   * `Foo10.bsa` (digit before the `0`) → none (explicit member, not a start)
pub(crate) fn numeric_sibling_paths(path: &str) -> Vec<String> {
    let lower = path.to_ascii_lowercase();
    let (stem, ext) = if let Some(s) = lower.strip_suffix(".bsa") {
        (&path[..s.len()], ".bsa")
    } else if let Some(s) = lower.strip_suffix(".ba2") {
        (&path[..s.len()], ".ba2")
    } else {
        return Vec::new();
    };

    let last = stem.chars().last();
    let prev = stem.chars().rev().nth(1);
    match last {
        // Series START `…0` (Skyrim `Textures0` / `Meshes0`): strip the `0`
        // and offer `…1`..`…9`. Guard against `…10` (digit before the `0`),
        // which is an explicit member, not a series start.
        Some('0') if !prev.is_some_and(|c| c.is_ascii_digit()) => {
            let base = &stem[..stem.len() - 1]; // drop the trailing ASCII '0'
            (1..=9u32).map(|n| format!("{base}{n}{ext}")).collect()
        }
        // Mid-series non-zero digit (`…2`): the user is being explicit — do
        // not auto-expand (avoids double-opening every numbered archive).
        Some(c) if c.is_ascii_digit() => Vec::new(),
        // No trailing digit (FNV `… Textures.bsa`): offer `…2`..`…9`.
        _ => (2..=9u32).map(|n| format!("{stem}{n}{ext}")).collect(),
    }
}
