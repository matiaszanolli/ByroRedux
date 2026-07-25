//! Bethesda Scaleform runtime profiles.

use anyhow::{anyhow, Result};
use ruffle_core::tag_utils::SwfMovie;

/// The Scaleform/ActionScript generation used by a Bethesda UI.
///
/// Both profiles run on the same Ruffle VM. The profile selects the
/// game-specific host ABI layered around that VM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleformProfile {
    /// Skyrim-family Scaleform menus running on AVM1 / ActionScript 2.
    SkyrimAvm1,
    /// Fallout 4-family Scaleform menus running on AVM2 / ActionScript 3.
    Fallout4Avm2,
}

impl ScaleformProfile {
    /// Detect the appropriate profile from a SWF's FileAttributes.
    pub fn detect(swf_data: &[u8]) -> Result<Self> {
        let movie = SwfMovie::from_data(swf_data, "file:///menu.swf".to_string(), None)
            .map_err(|error| anyhow!("Failed to parse SWF: {error}"))?;
        Ok(Self::from_movie(&movie))
    }

    pub(crate) fn from_movie(movie: &SwfMovie) -> Self {
        if movie.is_action_script_3() {
            Self::Fallout4Avm2
        } else {
            Self::SkyrimAvm1
        }
    }

    /// Whether this profile expects an AVM2 / ActionScript 3 movie.
    pub const fn is_avm2(self) -> bool {
        matches!(self, Self::Fallout4Avm2)
    }

    pub(crate) const fn external_interface_id(self) -> &'static str {
        match self {
            Self::SkyrimAvm1 => "byroredux-skyrim-ui",
            Self::Fallout4Avm2 => "byroredux-fallout4-ui",
        }
    }
}
