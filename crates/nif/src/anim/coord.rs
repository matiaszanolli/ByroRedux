//! Coordinate-space conversion helpers (Zup → Yup) for animation key
//! import.
//!
//! Pre-#1044 / TD3-002 this file owned a divergent copy of
//! `zup_to_yup_pos` + `zup_to_yup_quat` that missed the #333
//! normalise-after-swizzle fix (Shepperd quaternion extraction can
//! emit ~3.5%-off-unity outputs on degenerate-determinant inputs;
//! authored KF rotation keys can ship `±1e-4` off unity from
//! export-tool drift). The single source of truth now lives in
//! `byroredux_core::math::coord`; this file re-exports for backwards
//! compatibility of the `zup_to_yup_pos` / `zup_to_yup_quat` names
//! and to keep call-site rewrites minimal.

pub use byroredux_core::math::coord::{
    zup_to_yup_pos, zup_to_yup_quat_wxyz as zup_to_yup_quat,
};
