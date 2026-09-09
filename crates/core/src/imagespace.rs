//! Image-space modifier (IMAD) state — the canonical post-process parameter
//! block shared by the gameplay runtime and the renderer (#3861).
//!
//! This used to be two structs: `ImageSpaceModifierFrame` in
//! `byroredux-scripting` and `ImageSpaceModifierView` in `byroredux-renderer`,
//! identical field-for-field *and* default-for-default, bridged by a
//! hand-written 14-assignment copy in `byroredux/src/app_frame.rs`. The two
//! names implied a gameplay→renderer translation boundary; there was none, so
//! adding a fifteenth IMAD field took three edits and omitting the third was
//! silent — the field simply never reached the GPU.
//!
//! It lives here for the same reason `ecs::components::water` does: both
//! crates already depend on `byroredux-core` and neither depends on the other,
//! so this is the only place one definition can serve both.

/// Fully sampled image-space state for one frame.
///
/// Identity defaults, so a game or a frame with no active IMAD pays only the
/// shader's uniform branch. The identity values are not all zero — see
/// [`Self::default`] and `defaults_are_the_identity_transform`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageSpaceModifier {
    pub blur_radius_pixels: f32,
    pub double_vision_strength: f32,
    pub motion_blur_strength: f32,
    pub radial_blur_strength: f32,
    pub radial_blur_ramp_up: f32,
    pub radial_blur_start: f32,
    pub radial_blur_ramp_down: f32,
    pub radial_blur_down_start: f32,
    pub radial_blur_center: [f32; 2],
    pub saturation: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub tint_color: [f32; 4],
    pub fade_color: [f32; 4],
}

impl Default for ImageSpaceModifier {
    /// Exact identity: the renderer must be able to run this unconditionally
    /// and produce the un-modified image.
    ///
    /// Four of the fourteen are not zero, which is the trap this type's
    /// duplication used to multiply by three — `radial_blur_down_start: 1.0`
    /// (the ramp ends where it starts, so no ramp), `radial_blur_center` at
    /// screen centre, `tint_color` white with **zero alpha** (alpha is the
    /// blend weight, so the white is inert), and `saturation`/`brightness`/
    /// `contrast` at unity.
    fn default() -> Self {
        Self {
            blur_radius_pixels: 0.0,
            double_vision_strength: 0.0,
            motion_blur_strength: 0.0,
            radial_blur_strength: 0.0,
            radial_blur_ramp_up: 0.0,
            radial_blur_start: 0.0,
            radial_blur_ramp_down: 0.0,
            radial_blur_down_start: 1.0,
            radial_blur_center: [0.5, 0.5],
            saturation: 1.0,
            brightness: 1.0,
            contrast: 1.0,
            tint_color: [1.0, 1.0, 1.0, 0.0],
            fade_color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ImageSpaceModifier;

    /// #3861 — the identity defaults, pinned field by field.
    ///
    /// `Default` for this type is a *behavioural* claim, not a convenience:
    /// the renderer applies the block every frame, so a wrong default is a
    /// permanent, global image change rather than a missing feature. Four of
    /// the fourteen values are non-zero and none of them is guessable, which
    /// is exactly why keeping three copies in step by hand was a liability.
    #[test]
    fn defaults_are_the_identity_transform() {
        let d = ImageSpaceModifier::default();
        for (name, value) in [
            ("blur_radius_pixels", d.blur_radius_pixels),
            ("double_vision_strength", d.double_vision_strength),
            ("motion_blur_strength", d.motion_blur_strength),
            ("radial_blur_strength", d.radial_blur_strength),
            ("radial_blur_ramp_up", d.radial_blur_ramp_up),
            ("radial_blur_start", d.radial_blur_start),
            ("radial_blur_ramp_down", d.radial_blur_ramp_down),
        ] {
            assert_eq!(value, 0.0, "{name} must be inert by default");
        }
        assert_eq!(
            d.radial_blur_down_start, 1.0,
            "the radial-blur down-ramp must start at the far end, or every \
             frame gets an unauthored vignette"
        );
        assert_eq!(d.radial_blur_center, [0.5, 0.5], "screen centre");
        assert_eq!((d.saturation, d.brightness, d.contrast), (1.0, 1.0, 1.0));
        assert_eq!(
            d.tint_color,
            [1.0, 1.0, 1.0, 0.0],
            "tint is white with ZERO alpha — alpha is the blend weight, so the \
             white is inert; a default alpha of 1.0 would wash out every frame"
        );
        assert_eq!(d.fade_color, [0.0, 0.0, 0.0, 0.0]);
    }

    /// Anti-drift: the field count is part of the contract this type exists to
    /// enforce, since the failure it replaces was "someone added a field in
    /// one of three places".
    #[test]
    fn the_field_count_is_what_the_renderer_uploads() {
        let src = include_str!("imagespace.rs");
        let start = src
            .find("pub struct ImageSpaceModifier {")
            .expect("the struct must exist");
        let body = &src[start..start + src[start..].find("\n}").expect("struct body")];
        assert_eq!(
            body.matches("    pub ").count(),
            14,
            "ImageSpaceModifier gained or lost a field. That is fine — but the \
             renderer's uniform block and `presentation.rs`'s upload must move \
             with it, which is the lockstep this single definition exists to \
             make possible. Update this count deliberately."
        );
    }
}
