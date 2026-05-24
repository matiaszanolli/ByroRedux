//! Tiny argv parsers used by `App::new` and `main`. Free functions
//! split out of `main.rs` to stay below the 2000-LOC ceiling
//! (TD9-NEW-01 / #1267).

pub fn parse_string_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Parse `x,y,z` into a `(f32, f32, f32)` tuple — stored as a plain
/// triple here to avoid leaking the renderer's `Vec3` into main.rs.
pub fn parse_vec3_arg(args: &[String], flag: &str) -> Option<(f32, f32, f32)> {
    let s = parse_string_arg(args, flag)?;
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.as_slice() {
        [x, y, z] => Some((*x, *y, *z)),
        _ => {
            log::warn!("`{flag} {s}` could not be parsed as x,y,z floats; ignoring");
            None
        }
    }
}
