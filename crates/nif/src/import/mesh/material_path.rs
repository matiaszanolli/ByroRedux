//! `material_path_from_name` capture.
//!
//! Pulls a `.bgsm`/`.bgem` material path off a shape's name string when the
//! shader-property fallback would lose it.

use crate::blocks::shader::is_material_reference;

use byroredux_core::string::{FixedString, StringPool};

/// Only suffixed names (`.bgsm`/`.bgem`/`.mat`) become a material path.
///
/// #4439 — this is also a decision, not an accident, for Starfield's
/// suffix-less stub names. Vanilla authors exactly one such form: the bare
/// directory `Materials\` (384) / `\Materials` (3), on editor markers,
/// conveyor belts, pedestals — 387 of 480,861 stub names across 120,543
/// NIFs. It names no material, so it stays `None`: the merge reports it
/// `Unresolved` and the shape keeps its NIF-stub defaults. Routing it to
/// the CDB PBR fallback would stamp `external_material_resolved` on a
/// reference that resolves to nothing.
pub fn material_path_from_name(name: Option<&str>, pool: &mut StringPool) -> Option<FixedString> {
    let name = name?;
    if is_material_reference(name) {
        Some(pool.intern(name))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #4439 — the measured suffix-less Starfield stub names are bare
    /// directories and must not become material paths; real references
    /// still do.
    #[test]
    fn directory_only_starfield_references_capture_no_material_path() {
        let mut pool = StringPool::new();
        for name in ["Materials\\", "\\Materials"] {
            assert_eq!(material_path_from_name(Some(name), &mut pool), None, "{name}");
        }
        assert!(material_path_from_name(Some("Materials\\Foo\\Bar.mat"), &mut pool).is_some());
    }
}
