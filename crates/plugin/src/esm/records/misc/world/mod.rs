//! World-definition records: navigation, regions and encounter zones, keywords,
//! attraction rules and condition forms, lighting templates and image spaces,
//! activators and terminals — one child module per record family.

mod activator_terminal;
mod keyword;
mod lighting;
mod navigation;
mod region;

pub use activator_terminal::*;
pub use keyword::*;
pub use lighting::*;
pub use navigation::*;
pub use region::*;

/// Split `data` into fixed-size rows, ignoring a short trailing remainder.
///
/// Every `RDAT` payload observed in the corpus is an exact multiple of its row
/// size; truncating rather than erroring keeps one malformed plugin from
/// dropping an entire region.
pub(super) fn rows(data: &[u8], stride: usize) -> impl Iterator<Item = &[u8]> {
    data.chunks_exact(stride.max(1))
}

/// Strict sibling of [`rows`]: refuses a non-zero remainder instead of
/// silently discarding it. Mirrors the all-or-nothing posture
/// [`decode_nvgd`] and [`decode_nvnm`] already take — a `None` here should
/// make the caller treat the whole sub-record as unrecognised (an
/// empty/default field) rather than half-filled, so a stride that stops
/// matching a future game's data surfaces immediately instead of degrading
/// silently (#3404).
pub(super) fn rows_exact(data: &[u8], stride: usize) -> Option<impl Iterator<Item = &[u8]>> {
    if stride == 0 || !data.len().is_multiple_of(stride) {
        return None;
    }
    Some(data.chunks_exact(stride))
}

#[cfg(test)]
mod tests;
