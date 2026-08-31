//! Archive **naming** rules — the conventions Bethesda uses to split one
//! logical archive across a numbered series.
//!
//! Separate from the readers because the rule is consumed by two very
//! different callers: the engine's asset provider, which opens the siblings,
//! and the launcher's install validator, which must not report an absent
//! `…1.bsa` beside a present `…0.bsa` as missing. Pure and I/O-free, so both
//! agree by construction rather than by review.

/// Candidate numeric-sibling archive paths for an explicitly-named archive
/// (the primary `path` itself is excluded). Pure (no I/O) so the case logic —
/// the risky part — is unit-testable; the caller filters to existing files.
///
///   * `Foo.bsa`  (no trailing digit, FNV) → `Foo2.bsa` … `Foo9.bsa`
///   * `Foo0.bsa` (zero-based series start, Skyrim) → `Foo1.bsa` … `Foo9.bsa`
///   * `Foo01.bsa` (two-digit zero-padded series start, Starfield) →
///     `Foo02.bsa` … `Foo09.bsa`
///   * `Foo2.bsa` (mid-series digit) → none (the user lists members explicitly)
///   * `Foo10.bsa` (digit before the `0`) → none (explicit member, not a start)
pub fn numeric_sibling_paths(path: &str) -> Vec<String> {
    let lower = path.to_ascii_lowercase();
    let (stem, ext) = if let Some(s) = lower.strip_suffix(".bsa") {
        (&path[..s.len()], ".bsa")
    } else if let Some(s) = lower.strip_suffix(".ba2") {
        (&path[..s.len()], ".ba2")
    } else {
        return Vec::new();
    };

    // Starfield two-digit zero-padded series START (`…01`): strip the two
    // trailing digits and offer `…02`..`…09`. Guard against a longer digit
    // run before it (`…101` is an explicit 3-digit member, not a 2-digit
    // series start) the same way the single-`0` case guards against `…10`.
    let mut rev = stem.chars().rev();
    let (d0, d1, d2) = (rev.next(), rev.next(), rev.next());
    if d0 == Some('1') && d1 == Some('0') && !d2.is_some_and(|c| c.is_ascii_digit()) {
        let base = &stem[..stem.len() - 2];
        return (2..=9u32).map(|n| format!("{base}0{n}{ext}")).collect();
    }

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
