/// A game archive that can extract files by path.
/// Wraps either a BSA (Oblivion–Skyrim SE) or BA2 (FO4–Starfield) archive.
pub(crate) enum Archive {
    Bsa(byroredux_bsa::BsaArchive),
    Ba2(byroredux_bsa::Ba2Archive),
}

/// Read exactly the 4 magic bytes from `r` — the testable core of
/// [`Archive::open`]'s dispatch sniff. Split out so a byte-counting
/// `Read` wrapper can prove the caller never reads past this fixed
/// window, without needing a real (potentially multi-GB) file on disk.
/// See #2615 / SF-D3-03.
fn sniff_magic_from<R: std::io::Read>(mut r: R) -> std::io::Result<[u8; 4]> {
    let mut m = [0u8; 4];
    r.read_exact(&mut m)?;
    Ok(m)
}

impl Archive {
    /// Open an archive file, auto-detecting BSA vs BA2 from the file magic.
    pub(crate) fn open(path: &str) -> Result<Self, String> {
        // #2615 / SF-D3-03 — sample just the magic bytes instead of
        // `std::fs::read`ing the whole archive into a transient `Vec<u8>`.
        // Starfield's mesh archives run multi-GB; the old sniff allocated
        // and filled the entire file (twice per provider build, per
        // `build_material_provider`'s own doc — the archive is re-opened
        // for its file table via a second `Archive::open` call) purely to
        // read 4 bytes. `BsaArchive::open`/`Ba2Archive::open` below do
        // their own real (streamed) file-table read.
        let magic = std::fs::File::open(path)
            .and_then(sniff_magic_from)
            .map_err(|e| format!("read '{}': {}", path, e))?;
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

    /// Whether `path` is present, without extracting (and decompressing)
    /// it. Both backends answer from their in-memory file table, so this is
    /// a hash lookup — cheap enough for the per-quad availability probe the
    /// baked-LOD band selector runs every reconcile
    /// (`cell_loader::lod_bands`).
    pub(crate) fn contains(&self, path: &str) -> bool {
        match self {
            Archive::Bsa(a) => a.contains(path),
            Archive::Ba2(a) => a.contains(path),
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

    /// Find an entry by basename only, ignoring its stored folder path,
    /// case-insensitively. Unlike [`Self::list_files`] (deliberately blank
    /// for BSA — see that method's doc), this uses each backend's own real
    /// file table: `BsaArchive::list_files` and `Ba2Archive::list_files`
    /// both return every entry.
    ///
    /// #3555 — exists for exactly one caller, [`is_facegen_tool_path`]'s
    /// fallback in `texture.rs`. A handful of vanilla Oblivion head-part
    /// NIFs author a FaceGen SDK export-tool path instead of the real
    /// archive path; there is no cheap hashed key for "the file with this
    /// name, wherever it actually lives," so this does a full scan. Only
    /// call it on an already-confirmed miss from the normal `contains`/
    /// `extract` fast path — never as a general-purpose lookup.
    pub(crate) fn find_by_basename(&self, path: &str) -> Option<String> {
        let basename = path.rsplit(['\\', '/']).next().unwrap_or(path);
        let files: Vec<&str> = match self {
            Archive::Bsa(a) => a.list_files(),
            Archive::Ba2(a) => a.list_files(),
        };
        files
            .into_iter()
            .find(|f| {
                f.rsplit(['\\', '/'])
                    .next()
                    .unwrap_or(f)
                    .eq_ignore_ascii_case(basename)
            })
            .map(str::to_string)
    }
}

/// Whether a canonicalised texture path carries a FaceGen SDK
/// export-tool prefix instead of a real Data-relative one.
///
/// #3555 — three vanilla Oblivion head-part NIFs (`earshuman.nif`,
/// `earshighelf.nif`, `earswoodelf.nif`) author their shared ear texture as
/// `facegen\ears\human\EarsHuman.dds` — the FaceGen tool's own internal
/// export path, baked in uncorrected — instead of the archive's real
/// `textures\characters\imperial\earshuman.dds`. Verified against the
/// shipped BSAs directly (not guessed): unlike the `.spt` `trees\` case
/// (#3528), where `trees\` really is a top-level archive folder, there is
/// **no** `facegen\` folder anywhere in Oblivion's archives — rooting or
/// unrooting the prefix can never resolve it. [`TextureProvider`]'s
/// basename fallback is the correct recovery instead.
///
/// Checked on the already-canonicalised key (post [`normalize_texture_path`],
/// which always prepends `textures\` when missing) so it matches regardless
/// of whether the caller's `path` was the raw authored string or an
/// already-canonical one — see the two call sites in `texture.rs`.
pub(crate) fn is_facegen_tool_path(canonical: &str) -> bool {
    let bytes = canonical.as_bytes();
    bytes.len() >= 17 && bytes[..17].eq_ignore_ascii_case(b"textures\\facegen\\")
}

impl byroredux_ui::ScaleformResourceProvider for Archive {
    fn load(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        if self.contains(path) {
            self.extract(path).map(Some)
        } else {
            Ok(None)
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

/// The one canonical form a texture path takes before it is used as either a
/// bindless cache key or a BSA/BA2 lookup key.
///
/// Composition of the two independent canonicalisations that already existed:
/// [`strip_build_prefix`] (drop an *embedded* `\data\` build-server root) and
/// [`normalize_texture_path`] (drop a *leading* `data\`, then guarantee the
/// `textures\` root). Neither subsumes the other — `strip_build_prefix`
/// requires a separator before `data`, so a path that *starts* with `Data\`
/// falls straight through it.
///
/// #3334 — `resolve_texture_view_with_clamp` used to key on
/// `strip_build_prefix` alone while `TextureProvider::extract` looked up
/// through `normalize_texture_path`. Extraction therefore always succeeded,
/// but the key was wrong-shaped: every FNV `WATR.NNAM` authors
/// `Data\Textures\Water\WastelandWaterPotomac.dds`, which the registry's own
/// `normalize_path` then expanded to
/// `textures/data/textures/water/wastelandwaterpotomac.dds` — a duplicate
/// bindless slot and a duplicate GPU upload, and a cache entry the REFR walk
/// could never populate. Routing both through one function makes the two keys
/// agree by construction rather than by two call sites staying in step.
pub(crate) fn canonical_texture_key(path: &str) -> String {
    normalize_texture_path(&strip_build_prefix(path)).into_owned()
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

/// The numeric-sibling naming rule, re-exported from `byroredux-bsa`.
///
/// Moved out of this module (#launcher P1) so the launcher's install
/// validator can apply the *same* rule when deciding whether an absent
/// `…1.bsa` beside a present `…0.bsa` is a finding. Two implementations of
/// "which archives does this one drag in" would let the validator disagree
/// with the loader, which is worse than having no validator at all.
pub(crate) use byroredux_bsa::numeric_sibling_paths;

#[cfg(test)]
mod tests {
    use super::sniff_magic_from;
    use std::io::Read;

    /// Wraps a `Read` and counts every byte actually pulled through it —
    /// the "byte-counting reader wrapper" from #2615's completeness
    /// check. Lets the test prove the magic sniff never reads past its
    /// fixed 4-byte window without needing a real multi-GB file on disk.
    struct CountingReader<R> {
        inner: R,
        bytes_read: usize,
    }

    impl<R: Read> Read for CountingReader<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.bytes_read += n;
            Ok(n)
        }
    }

    /// Regression: #2615 / SF-D3-03 — pre-fix, `Archive::open` sniffed
    /// the dispatch magic via `std::fs::read`, pulling the ENTIRE file
    /// through before ever looking at the first 4 bytes. A 100 MB
    /// in-memory stand-in (no real file needed — the point is proving
    /// the *reader*, not exercising the filesystem) with a counting
    /// wrapper confirms the fixed-size sniff reads exactly 4 bytes
    /// regardless of how much data follows.
    #[test]
    fn magic_sniff_reads_at_most_four_bytes_from_a_huge_source() {
        let mut source = vec![0x42u8; 100 * 1024 * 1024]; // 100 MB
        source[0..4].copy_from_slice(b"BTDX");
        let mut counting = CountingReader {
            inner: std::io::Cursor::new(&source),
            bytes_read: 0,
        };
        let magic = sniff_magic_from(&mut counting).expect("100 MB source has plenty of bytes");
        assert_eq!(&magic, b"BTDX");
        assert_eq!(
            counting.bytes_read, 4,
            "the magic sniff must read exactly 4 bytes, not the whole 100 MB source"
        );
    }

    /// A source shorter than 4 bytes fails — same "too small" rejection
    /// the pre-fix `data.len() < 4` check enforced, now surfaced as a
    /// bounds-checked `UnexpectedEof` from `read_exact` instead.
    #[test]
    fn magic_sniff_errors_on_a_too_short_source() {
        let source: &[u8] = b"ab";
        assert!(sniff_magic_from(source).is_err());
    }
}
