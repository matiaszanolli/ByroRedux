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

/// Known-good stream-drift values for the five `bhk*Constraint` types
/// with typed CInfo decoders (see `is_havok_constraint_stub`'s #3713
/// note in `lib.rs` — `bhkRagdollConstraint`, `bhkLimitedHingeConstraint`,
/// `bhkHingeConstraint`, `bhkMalleableConstraint`,
/// `bhkPrismaticConstraint`) — the by-design "motor left for `block_size`
/// recovery" tail, characterised byte-for-byte against nif.xml's
/// `bhkConstraintMotorCInfo` (1-byte `hkMotorType` discriminator +
/// conditional payload):
///
/// | drift | composition |
/// |------:|-------------|
/// |     1 | motor type byte, `MOTOR_NONE` (no payload) |
/// |    18 | motor type byte + `bhkSpringDamperConstraintMotor` (17 B) |
/// |    19 | motor type byte + `bhkLimitedForceConstraintMotor` (18 B, `MOTOR_VELOCITY`) |
/// |    26 | motor type byte + `bhkPositionConstraintMotor` (25 B) |
///
/// `bhkMalleableConstraint`'s own residual additionally carries the
/// trailing `Strength: f32` (4 B) the malleable wrapper itself leaves
/// unread, stacked on top of its WRAPPED inner type's own motor-tail
/// drift — 0 for an inner type with no motor field at all
/// (`bhkBallAndSocketConstraint` / `bhkStiffSpringConstraint`, the two
/// non-motor arms `parse_fo3_malleable_inner` byte-skips), or one of the
/// four values above for an inner Ragdoll/LimitedHinge/Hinge/Prismatic.
/// So Malleable's own observed set is `{4}` (no-motor inner) ∪
/// `{5, 22, 23, 30}` (motor-tail-drift + 4) — verified against a real FO3
/// corpus (`Fallout - Meshes.bsa`): 59 instances at +5, 1 at +4.
///
/// Used by the corpus-facing drift assertion
/// (`tests/constraint_drift_corpus.rs`) to turn an unexpected residual —
/// like the historic `bhkHingeConstraint` +128 (a whole undecoded CInfo,
/// not a motor tail) that #3330 found only by hand — into a hard failure
/// instead of a value nothing inspects. `nif_stats --drift-histogram`
/// (#939) is the human-facing sibling for spot-checking a corpus by eye.
pub fn is_known_constraint_motor_tail_drift(type_name: &str, drift: i64) -> bool {
    const MOTOR_TAIL_DRIFTS: [i64; 4] = [1, 18, 19, 26];
    if type_name == "bhkMalleableConstraint" {
        return drift == 4 || MOTOR_TAIL_DRIFTS.iter().any(|&d| drift == d + 4);
    }
    MOTOR_TAIL_DRIFTS.contains(&drift)
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

    /// #3713 — the four motor-tail values, on any decoded constraint type.
    #[test]
    fn known_motor_tail_drifts_are_accepted_for_any_decoded_type() {
        for ty in [
            "bhkRagdollConstraint",
            "bhkLimitedHingeConstraint",
            "bhkHingeConstraint",
            "bhkPrismaticConstraint",
        ] {
            for &drift in &[1, 18, 19, 26] {
                assert!(
                    is_known_constraint_motor_tail_drift(ty, drift),
                    "{ty} drift={drift} should be a known motor-tail value"
                );
            }
        }
    }

    /// `bhkMalleableConstraint`'s own residual is the *sum* of the trailing
    /// `Strength: f32` (4 B, its own wrapper leaves unread) and its wrapped
    /// inner type's motor-tail drift — 0 for a non-motor inner
    /// (`+4` alone), or one of the four base values `+4` for a
    /// motor-bearing inner. Verified against a real FO3 corpus: 59
    /// `bhkMalleableConstraint` instances at +5 (= 1 + 4), 1 at +4.
    #[test]
    fn malleable_accepts_the_strength_trailer_stacked_on_the_inner_motor_tail() {
        // No-motor inner (BallAndSocket / StiffSpring): Strength alone.
        assert!(is_known_constraint_motor_tail_drift(
            "bhkMalleableConstraint",
            4
        ));
        // Motor-bearing inner: each base value + 4.
        for &drift in &[5, 22, 23, 30] {
            assert!(
                is_known_constraint_motor_tail_drift("bhkMalleableConstraint", drift),
                "bhkMalleableConstraint drift={drift} should be a known Strength+motor-tail value"
            );
        }
        // The BARE motor-tail values (no +4) must NOT be accepted for
        // Malleable — its wrapper always adds the Strength trailer on top.
        for &drift in &[1, 18, 19, 26] {
            assert!(
                !is_known_constraint_motor_tail_drift("bhkMalleableConstraint", drift),
                "bhkMalleableConstraint drift={drift} is a bare motor-tail value, missing \
                 the +4 Strength trailer every Malleable residual carries"
            );
        }
    }

    /// +4 (and its `+motor` variants) are Malleable-specific — a bare
    /// decoded type (no `Strength` trailer) must not accept +4, and the
    /// historic `bhkHingeConstraint` +128 (a whole undecoded CInfo, not a
    /// motor tail) must never be mistaken for a known value on any type.
    #[test]
    fn unknown_or_wrong_type_drifts_are_rejected() {
        assert!(!is_known_constraint_motor_tail_drift(
            "bhkRagdollConstraint",
            4
        ));
        assert!(!is_known_constraint_motor_tail_drift(
            "bhkHingeConstraint",
            128
        ));
        assert!(!is_known_constraint_motor_tail_drift(
            "bhkMalleableConstraint",
            128
        ));
    }
}
