//! `material_path_from_name` capture.
//!
//! Pulls a `.bgsm`/`.bgem` material path off a shape's name string when the
//! shader-property fallback would lose it.



use crate::blocks::shader::is_material_reference;

use byroredux_core::string::{FixedString, StringPool};

pub fn material_path_from_name(
    name: Option<&str>,
    pool: &mut StringPool,
) -> Option<FixedString> {
    let name = name?;
    if is_material_reference(name) {
        Some(pool.intern(name))
    } else {
        None
    }
}

