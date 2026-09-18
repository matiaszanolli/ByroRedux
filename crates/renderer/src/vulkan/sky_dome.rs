//! Invariants for the shared sky dome (`shaders/include/sky.glsl`).
//!
//! The analytic sky used to live inside `composite.frag` and read that
//! pass's `CompositeParams` block directly, which meant a second consumer
//! could only exist by copying ~400 lines of gradient / cloud / celestial
//! logic. It now lives in `include/sky.glsl` behind a `SkyDome` value, so
//! the background pass and any bake (the sky cubemap) run one
//! implementation.
//!
//! That seam has one failure mode worth a guard. `SkyDome` is filled by a
//! per-consumer builder that copies field-for-field out of whatever
//! uniform block that consumer happens to have. GLSL neither
//! zero-initialises a local struct nor warns about a field you forgot, so
//! a builder that misses one hands the sky an **uninitialised** `vec4` and
//! the result is undefined — not a compile error, not a warning, and on
//! most drivers not even obviously wrong at first glance. The check below
//! is a completeness enumeration over the struct's own field list, so
//! adding a field to `SkyDome` fails every builder that has not been
//! updated.
//!
//! There is no runtime state here; the sky dome is pure shader code. This
//! module exists for the tests.

/// Field names declared by `struct SkyDome` in `include/sky.glsl`.
///
/// Parsed from the shader rather than restated, so the list cannot drift
/// from the declaration it is supposed to describe.
#[cfg(test)]
pub(crate) fn sky_dome_fields(sky_glsl: &str) -> Vec<String> {
    let body = sky_glsl
        .split_once("struct SkyDome {")
        .expect("include/sky.glsl still declares `struct SkyDome`")
        .1
        .split_once("};")
        .expect("the SkyDome declaration is still terminated")
        .0;
    body.lines()
        .filter_map(|line| {
            let line = line.trim();
            // Skip comments and blank lines; every real member is `vec4 name;`.
            let rest = line.strip_prefix("vec4 ")?;
            let name = rest.split(';').next()?.trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKY_GLSL: &str = include_str!("../../shaders/include/sky.glsl");
    const COMPOSITE: &str = include_str!("../../shaders/composite.frag");

    /// Sanity on the parser itself — if it silently matched nothing, every
    /// completeness check below would pass vacuously.
    #[test]
    fn the_sky_dome_declares_the_fields_the_sky_actually_reads() {
        let fields = sky_dome_fields(SKY_GLSL);
        assert!(
            fields.len() >= 19,
            "parsed only {} SkyDome fields — the parser has lost the declaration",
            fields.len(),
        );
        // Every `dome.<field>` the shader reads must be a declared member.
        // Catches a rename that updates the reads but not the struct.
        let mut read: Vec<&str> = SKY_GLSL
            .match_indices("dome.")
            .map(|(at, _)| {
                let rest = &SKY_GLSL[at + "dome.".len()..];
                let end = rest
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .collect();
        read.sort_unstable();
        read.dedup();
        for name in read {
            assert!(
                fields.iter().any(|f| f == name),
                "sky.glsl reads `dome.{name}` but SkyDome declares no such field",
            );
        }
    }

    /// Every consumer's builder must assign every `SkyDome` field.
    ///
    /// A GLSL local struct is uninitialised; a forgotten field is undefined
    /// behaviour rather than a diagnostic. This is the whole reason the
    /// struct-parameterised seam needs a guard at all.
    #[test]
    fn every_sky_dome_builder_assigns_every_field() {
        let fields = sky_dome_fields(SKY_GLSL);
        // (shader name, source, builder function name). Add a row when a
        // new consumer grows its own builder — the sky cubemap bake will.
        for (shader, src, builder) in [("composite.frag", COMPOSITE, "SkyDome build_sky_dome()")] {
            let body = src
                .split_once(builder)
                .unwrap_or_else(|| panic!("{shader} no longer defines `{builder}`"))
                .1
                .split_once("return dome;")
                .unwrap_or_else(|| panic!("{shader}'s builder no longer returns `dome`"))
                .0;
            for field in &fields {
                assert!(
                    body.contains(&format!("dome.{field} =")),
                    "{shader}'s builder never assigns `dome.{field}` — GLSL leaves it \
                     uninitialised and the sky reads undefined data",
                );
            }
        }
    }

    /// The point of the extraction: exactly one implementation. If the sky
    /// functions reappear in a consumer, the include has been bypassed.
    #[test]
    fn composite_does_not_carry_its_own_copy_of_the_sky() {
        assert!(
            COMPOSITE.contains("#include \"include/sky.glsl\""),
            "composite.frag must consume the shared sky dome, not its own copy",
        );
        for gone in [
            "vec3 compute_sky(",
            "vec4 weather_procedural_cloud(",
            "vec3 weather_sky_details(",
            "float weather_star_field(",
        ] {
            assert!(
                !COMPOSITE.contains(gone),
                "composite.frag re-declares `{gone}` — the sky has been forked back \
                 out of include/sky.glsl",
            );
        }
    }

    /// Every shader reading `weather_wind` must match the host packing,
    /// `[dir.x, speed, dir.z, 0]`. The field's own comments once said
    /// "dir x/z, normalized speed", which reads as `.xy` / `.z`; the cloud
    /// march took it that way and advected with speed folded into the
    /// direction. The authored-plane path read it correctly, so the two
    /// cloud representations drifted in different directions.
    #[test]
    fn wind_consumers_match_the_host_packing() {
        let draw = include_str!("context/draw.rs");
        let packing = draw
            .split_once("weather_wind: [")
            .expect("build_composite_params still packs weather_wind")
            .1
            // The closing bracket on its own line — a bare `"],"` would stop
            // inside `wind_direction[0],`.
            .split_once("\n        ],")
            .expect("that packing is still terminated")
            .0;
        let lanes: Vec<&str> = packing
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(
            lanes,
            [
                "sky_params.weather.wind_direction[0],",
                "sky_params.weather.wind_speed,",
                "sky_params.weather.wind_direction[1],",
                "0.0,",
            ],
            "the host weather_wind packing changed — every shader swizzle below is \
             now suspect",
        );

        let clouds = include_str!("../../shaders/include/clouds.glsl");
        assert!(
            clouds.contains("dome.weather_wind.xz * dome.weather_wind.y"),
            "clouds.glsl must advect along .xz at speed .y",
        );
        for bad in ["weather_wind.xy", "weather_wind.z "] {
            assert!(
                !clouds.contains(bad),
                "clouds.glsl reads `{bad}`, which is not how the host packs wind",
            );
        }
        // The 2D procedural body that was the second wind reader is gone. If
        // a new reader appears in sky.glsl it must read the same lanes.
        for bad in ["weather_wind.xy", "weather_wind.z "] {
            assert!(
                !SKY_GLSL.contains(bad),
                "sky.glsl reads `{bad}`, which is not how the host packs wind",
            );
        }
    }

    /// W3.15 (light & shadow campaign) — the SH sky-ambient fallback is
    /// deliberately unoccluded (nine SH coefficients cannot carry local
    /// visibility), so the occlusion duty lives on the CONSUMERS. Pin both
    /// diffuse sites' gates: the primary fragment ambient must keep
    /// multiplying by `combinedAO`, and the blade ambient by the §12.1
    /// `skyVisibility` term. The `exteriorSkyRadianceOr` miss sites have no
    /// gate BY DESIGN (an escaped ray genuinely sees sky) — if a new
    /// DIFFUSE consumer of `exteriorSkyDiffuseOr` appears, it must gate on
    /// an occlusion term or extend this pin.
    #[test]
    fn sky_visibility_pin() {
        let triangle = include_str!("../../shaders/triangle.frag");
        let blades = include_str!("../../shaders/groundcover_blade.frag");
        let bindings = include_str!("../../shaders/include/bindings.glsl");
        assert!(
            bindings.contains("vec3 exteriorSkyDiffuseOr(vec3 normal, vec3 fallback)"),
            "the SH diffuse accessor signature moved — re-check the consumers below"
        );
        let gate = "exteriorSkyDiffuseOr(N, vec3(0.0)) * primaryDiffuseWeight;";
        let pos = triangle
            .find(gate)
            .expect("triangle.frag's SH diffuse consumer moved");
        let tail = &triangle[pos..pos + 220];
        assert!(
            tail.contains("* combinedAO"),
            "triangle.frag's SH sky-diffuse ambient must stay multiplied by \
             combinedAO — the SH fallback is unoccluded by construction and \
             enclosed exterior geometry reads unoccluded sky without this \
             gate (skyal.md §Diffuse exterior illumination, W3.15)"
        );
        let bpos = blades
            .find("exteriorSkyDiffuseOr(N, sceneFlags.yzw)")
            .expect("groundcover_blade.frag's SH ambient consumer moved");
        let btail = &blades[bpos..bpos + 120];
        assert!(
            btail.contains("skyVisibility"),
            "the blade SH ambient must stay gated by §12.1's skyVisibility — \
             without it a dense sward's base glows with unoccluded sky"
        );
    }

    /// `sky.glsl` owns the set-1 bindless array (it is the only thing in
    /// either consumer that samples it). Two declarations of one binding in
    /// the same stage is a compile error, so a consumer that re-declares it
    /// breaks the build — but only for whoever includes both, which may not
    /// be the person who added the line.
    #[test]
    fn the_bindless_array_is_declared_once_by_the_include() {
        assert!(
            SKY_GLSL.contains("layout(set = 1, binding = 0) uniform sampler2D textures[];"),
            "include/sky.glsl must declare the bindless texture array it samples",
        );
        assert!(
            !COMPOSITE.contains("uniform sampler2D textures[]"),
            "composite.frag must not re-declare the set-1 bindless array — \
             include/sky.glsl owns it",
        );
    }
}
