//! Shared conventions for walking a game archive as a NIF corpus.
//!
//! Both the `nif_stats` example and the `tests/common` baseline harness walk
//! archive entries and emit the same per-block TSV. They had grown independent
//! copies of the two rules that must agree for a baseline to mean anything —
//! *which entries count as NIFs* and *what the TSV header looks like* — and
//! both copies drifted (#2587, #2347). They live here so there is one of each.

/// Archive-entry extensions that are NIFs.
///
/// `.bto` and `.btr` are **renamed NIFs**, not a separate format: Bethesda's
/// distant-LOD pipeline emits them through the identical `parse_nif` →
/// `import_nif_scene` path. Filtering on `.nif` alone (#2587) left 10,662
/// files in `Skyrim - Meshes1.bsa` alone — 3.3× that archive's `.nif` count —
/// contributing nothing to any baseline, so a parser change that broke Skyrim
/// distant-LOD geometry would have passed the full corpus gate silently.
///
/// * `.bto` — "block terrain object", a merged per-cell LOD mesh.
/// * `.btr` — "block terrain rock", the LOD rock/clutter companion.
pub const NIF_ENTRY_EXTENSIONS: &[&str] = &[".nif", ".bto", ".btr"];

/// Whether an archive entry should be parsed as a NIF.
///
/// Case-insensitive: BSA/BA2 path casing is not normalised and vanilla
/// archives are inconsistent about it.
pub fn is_nif_entry(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    NIF_ENTRY_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// The canonical `#`-prefixed header line for a per-block histogram TSV,
/// **without** its trailing newline.
///
/// #2347 — `nif_stats --tsv` and `PerBlockHistogram::to_tsv` maintained two
/// different header strings for the same file format. Readers skip `#` lines,
/// so nothing was broken, but two implementations of one format is friction
/// for anyone hand-diffing tool output against a checked-in baseline. This is
/// the one implementation; both call it.
///
/// `clean_truncated` is `Some((clean, truncated))` for the tool, which tracks
/// those counts, and `None` for the harness, which does not. Emitting the keys
/// with a fabricated `0` would be worse than omitting them.
pub fn per_block_tsv_header(total: usize, clean_truncated: Option<(usize, usize)>) -> String {
    match clean_truncated {
        Some((clean, truncated)) => format!(
            "# nif_stats per-block histogram\ttotal={total}\tclean={clean}\ttruncated={truncated}"
        ),
        None => format!("# nif_stats per-block histogram\ttotal={total}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distant_lod_extensions_are_nif_entries() {
        // #2587 — the whole point: these are NIFs and were being skipped.
        assert!(is_nif_entry("meshes/terrain/tamriel/tamriel.4.-32.-32.bto"));
        assert!(is_nif_entry("meshes/terrain/tamriel/objects/x.btr"));
        assert!(is_nif_entry("meshes/clutter/barrel01.nif"));
    }

    #[test]
    fn casing_does_not_matter() {
        assert!(is_nif_entry("MESHES\\TERRAIN\\TAMRIEL.BTO"));
        assert!(is_nif_entry("Meshes\\Clutter\\Barrel01.Nif"));
    }

    #[test]
    fn non_nif_entries_are_rejected() {
        assert!(!is_nif_entry("textures/clutter/barrel01.dds"));
        assert!(!is_nif_entry(
            "meshes/actors/character/behaviors/0_master.hkx"
        ));
        // A substring match, not a suffix one, would accept this.
        assert!(!is_nif_entry("meshes/nif_notes.txt"));
    }

    /// Whether `line` is a hand-rolled ".is this a NIF" test, in either
    /// spelling: a suffix match (`ends_with(".nif")`) or an extension compare
    /// against `"nif"` (`.extension()` … `eq_ignore_ascii_case("nif")`, `== "nif"`,
    /// `!= "nif"`). #5440 — #5259's scan knew only the first, so `nif_stats`'
    /// directory walker kept the second and silently dropped every `.bto` /
    /// `.btr` LOD mesh from a loose corpus.
    fn hand_rolled_nif_filter(line: &str) -> bool {
        line.contains("ends_with(\".nif\")")
            || line.contains("eq_ignore_ascii_case(\"nif\")")
            || line.contains("== \"nif\"")
            || line.contains("!= \"nif\"")
    }

    /// #5440 — both spellings are caught, and the sanctioned call is not.
    #[test]
    fn the_corpus_scan_knows_both_hand_rolled_spellings() {
        assert!(hand_rolled_nif_filter("if p.ends_with(\".nif\") {"));
        assert!(hand_rolled_nif_filter(".map(|s| s.eq_ignore_ascii_case(\"nif\"))"));
        assert!(hand_rolled_nif_filter("if lower != \"nif\" {"));
        assert!(hand_rolled_nif_filter("ext == \"nif\""));
        assert!(!hand_rolled_nif_filter("corpus::is_nif_entry(&path.to_string_lossy())"));
        assert!(!hand_rolled_nif_filter("let kind = \"nifskope\";"));
    }

    /// #5259 — the corpus definition lives once, in this file. A
    /// hand-rolled `ends_with(".nif")` filter under `tests/` or
    /// `examples/` silently drops every renamed-NIF LOD entry (22k+
    /// `.bto`/`.btr` files on FO76 alone) from a census or a baseline —
    /// the #2587/#4154/#4629 failure shape, reintroduced in-window by
    /// `sf_matpath_dump` (2026-09-30) after #4629's sweep. The scan
    /// rejects the hand-rolled spelling; the two exempt files carry
    /// deliberate `.nif ∪ .kf` scopes that `is_nif_entry` (NIF-only by
    /// design — KF clips are animation, not meshes) cannot express.
    #[test]
    fn corpus_filters_route_through_is_nif_entry() {
        const EXEMPT: &[(&str, &str)] = &[
            ("float_channel_survey.rs", "the .nif ∪ .kf animation survey"),
            (
                "parse_real_nifs.rs",
                "the torch-folder scope and the .nif ∪ .kf archive census",
            ),
        ];

        let crate_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut violations = Vec::new();
        for dir in ["tests", "examples"] {
            let mut stack = vec![crate_root.join(dir)];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).expect("readable tests/examples dir") {
                    let path = entry.expect("readable dir entry").path();
                    if path.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                        continue;
                    }
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if EXEMPT.iter().any(|(exempt, _)| *exempt == name) {
                        continue;
                    }
                    let src = std::fs::read_to_string(&path).expect("readable source");
                    for (i, line) in src.lines().enumerate() {
                        if hand_rolled_nif_filter(line) {
                            violations.push(format!(
                                "{}:{}  hand-rolled .nif filter — route through \
                                 corpus::is_nif_entry",
                                path.display(),
                                i + 1
                            ));
                        }
                    }
                }
            }
        }
        assert!(
            violations.is_empty(),
            "the .nif-only corpus rule was hand-rolled again — .bto/.btr LOD \
             entries silently drop out of that census (#2587/#4154/#4629):\n{}",
            violations.join("\n")
        );
    }

    #[test]
    fn header_forms_share_the_total_key() {
        let with = per_block_tsv_header(10, Some((8, 2)));
        let without = per_block_tsv_header(10, None);
        assert!(with.starts_with("# nif_stats per-block histogram\ttotal=10"));
        assert_eq!(without, "# nif_stats per-block histogram\ttotal=10");
        assert!(with.ends_with("\tclean=8\ttruncated=2"));
        // Neither form may carry a trailing newline — callers add their own,
        // and a stray one silently produces a blank row in the TSV.
        assert!(!with.contains('\n') && !without.contains('\n'));
    }
}
