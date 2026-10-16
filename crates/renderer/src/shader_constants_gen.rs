// Generator for the GLSL header `build.rs` writes from
// `shader_constants_data.rs` (#5097, split out by #5808). Like the data file,
// it is `include!`d by both:
//   - crates/renderer/src/shader_constants.rs   (library)
//   - crates/renderer/build.rs                   (build script)
// and must be included BEFORE the data file: the data file's table uses the
// row macros below, and `macro_rules!` is in scope only after its definition.

/// A [`HeaderLine::Define`] row that names its constant once (#5808): the
/// header name is `stringify!`d from the identifier the value reads, so a row
/// cannot emit one constant's value under another constant's name.
/// `shader_define!(Uint NAME)`, `shader_define!(Vec3 NAME)`, and
/// `shader_define!(Uint NAME as u32)` for a `usize` constant.
macro_rules! shader_define {
    (Vec3 $name:ident) => {
        HeaderLine::Define(stringify!($name), ShaderValue::Vec3(&$name))
    };
    (Vec2 $name:ident) => {
        HeaderLine::Define(stringify!($name), ShaderValue::Vec2(&$name))
    };
    ($kind:ident $name:ident as $ty:ty) => {
        HeaderLine::Define(stringify!($name), ShaderValue::$kind($name as $ty))
    };
    ($kind:ident $name:ident) => {
        HeaderLine::Define(stringify!($name), ShaderValue::$kind($name))
    };
}

/// The [`HeaderLine::Overridable`] counterpart of `shader_define!`.
macro_rules! shader_overridable {
    ($kind:ident $name:ident) => {
        HeaderLine::Overridable(stringify!($name), ShaderValue::$kind($name))
    };
}

/// One value shape a GLSL `#define` can carry, with the emission format
/// bound to the variant so the `u` suffix / float `{:?}` decision is made
/// once, here, not re-typed at an emitter and a pin list.
#[derive(Clone, Copy)]
pub enum ShaderValue {
    /// `{NAME}u` — a `u32` const, uint literal.
    Uint(u32),
    /// `{NAME}` — a `u32` const emitted without the suffix: workgroup-size
    /// and `layout(local_size_x = …)` / `#if` consumers take plain ints
    /// (#1758, #3308).
    UintPlain(u32),
    /// `{NAME:?}` — an `f32` const, always with a decimal point.
    Float(f32),
    /// `vec3({NAME[0]:?}, {NAME[1]:?}, {NAME[2]:?})` — an `[f32; 3]` const.
    Vec3(&'static [f32; 3]),
    /// `vec2({NAME[0]:?}, {NAME[1]:?})` — an `[f32; 2]` const.
    Vec2(&'static [f32; 2]),
}

/// One line of the generated header, in emit order. The variants cover
/// every line shape `shader_constants.glsl` has (#5097).
#[derive(Clone, Copy)]
pub enum HeaderLine {
    /// `#define NAME value`
    Define(&'static str, ShaderValue),
    /// `#ifndef NAME` / `#define NAME value` / `#endif` — a define the
    /// compile command may override (RT_COMPILE_ABLATION_MASK's A/B gate).
    Overridable(&'static str, ShaderValue),
    /// The #2978 raw-output block: catalog-derived `DBG_VIZ_RAW_OUTPUT_ANY_MASK`
    /// plus the `DBG_VIZ_REQUIRES_RAW_OUTPUT(flags)` predicate macro, folded
    /// from [`DBG_VIZ_RAW_OUTPUT_ANY`] / [`DBG_VIZ_RAW_OUTPUT_ALL`] so the
    /// header, the shaders and the Rust predicate cannot drift apart.
    DbgVizRequiresRawOutput,
    /// A `//` comment line (section headers, per-constant rationale).
    Comment(&'static str),
    /// A blank line.
    Blank,
}

fn push_define_line(out: &mut String, name: &str, value: &ShaderValue) {
    let rendered = match value {
        ShaderValue::Uint(v) => format!("{v}u"),
        ShaderValue::UintPlain(v) => format!("{v}"),
        ShaderValue::Float(v) => format!("{v:?}"),
        ShaderValue::Vec3(v) => format!("vec3({:?}, {:?}, {:?})", v[0], v[1], v[2]),
        ShaderValue::Vec2(v) => format!("vec2({:?}, {:?})", v[0], v[1]),
    };
    out.push_str("#define ");
    out.push_str(name);
    out.push(' ');
    out.push_str(&rendered);
    out.push('\n');
}

impl HeaderLine {
    /// Append this line (or block) to `out`, exactly as the header carries it.
    pub fn render(&self, out: &mut String) {
        match self {
            HeaderLine::Define(name, value) => push_define_line(out, name, value),
            HeaderLine::Overridable(name, value) => {
                out.push_str("#ifndef ");
                out.push_str(name);
                out.push('\n');
                push_define_line(out, name, value);
                out.push_str("#endif\n");
            }
            HeaderLine::DbgVizRequiresRawOutput => {
                out.push_str("// Debug views that must bypass the post-transport frame graph.\n");
                let names: Vec<&str> =
                    DBG_VIZ_RAW_OUTPUT_ANY.iter().map(|(name, _)| *name).collect();
                out.push_str(&format!("// Any-of: {}\n", names.join(" | ")));
                out.push_str(&format!(
                    "#define DBG_VIZ_RAW_OUTPUT_ANY_MASK {}u\n",
                    dbg_viz_raw_output_any_mask()
                ));
                let mut predicate = String::from(
                    "(((flags) & DBG_VIZ_RAW_OUTPUT_ANY_MASK) != 0u",
                );
                for (name, _) in DBG_VIZ_RAW_OUTPUT_ALL {
                    predicate.push_str(&format!(" || ((flags) & {name}) == {name}"));
                }
                predicate.push(')');
                out.push_str("#define DBG_VIZ_REQUIRES_RAW_OUTPUT(flags) ");
                out.push_str(&predicate);
                out.push('\n');
            }
            HeaderLine::Comment(text) => {
                out.push_str("// ");
                out.push_str(text);
                out.push('\n');
            }
            HeaderLine::Blank => out.push('\n'),
        }
    }
}

/// Render the whole `shaders/include/shader_constants.glsl` from
/// [`SHADER_DEFINES`]. `build.rs` writes this; the byte-exact pin test
/// re-renders it and compares against the committed file — there is no
/// third copy of the emit format to drift.
pub fn render_shader_constants_header() -> String {
    let mut out = String::with_capacity(24 * 1024);
    out.push_str("// AUTO-GENERATED by crates/renderer/build.rs — DO NOT EDIT\n");
    out.push_str("// Source of truth: crates/renderer/src/shader_constants_data.rs\n");
    out.push_str("// Regenerate:   cargo build -p byroredux-renderer\n");
    out.push_str(
        "// Then recompile shaders (from crates/renderer/shaders/): \
glslangValidator -V -I. <shader> -o <shader>.spv\n",
    );
    out.push_str("//\n");
    out.push_str(
        "// Integer #defines carry the `u` suffix (uint literals); floats always\n",
    );
    out.push_str("// have a decimal point. Workgroup-size #defines omit the suffix so they\n");
    out.push_str(
        "// work in layout(local_size_x = WORKGROUP_X) qualifiers.\n",
    );
    out.push('\n');
    out.push_str("#ifndef BYRO_SHADER_CONSTANTS_GLSL\n");
    out.push_str("#define BYRO_SHADER_CONSTANTS_GLSL\n");
    for line in SHADER_DEFINES {
        line.render(&mut out);
    }
    out.push_str("#endif // BYRO_SHADER_CONSTANTS_GLSL\n");
    out
}
