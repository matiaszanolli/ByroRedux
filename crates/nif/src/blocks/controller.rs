//! NiTimeController — animation controller stub.
//!
//! Parsed enough to read past the data correctly, but not interpreted
//! until the animation phase.

use crate::stream::NifStream;
use crate::types::BlockRef;
use super::NiObject;
use std::any::Any;
use std::io;

/// Stub for animation controller blocks.
/// Parses the base NiTimeController fields so the stream advances correctly.
#[derive(Debug)]
pub struct NiTimeController {
    pub next_controller_ref: BlockRef,
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub target_ref: BlockRef,
}

impl NiObject for NiTimeController {
    fn block_type_name(&self) -> &'static str {
        "NiTimeController"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTimeController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let next_controller_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let frequency = stream.read_f32_le()?;
        let phase = stream.read_f32_le()?;
        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;
        let target_ref = stream.read_block_ref()?;

        Ok(Self {
            next_controller_ref,
            flags,
            frequency,
            phase,
            start_time,
            stop_time,
            target_ref,
        })
    }
}
