//! Saved New Vegas Hardcore selection. This is a mode flag, not an
//! implementation of hunger, thirst, sleep, or the rest of Hardcore gameplay.
use crate::ecs::Resource;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct HardcoreMode {
    pub enabled: bool,
}

impl Resource for HardcoreMode {}
