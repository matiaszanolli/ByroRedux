//! Animation clip registry (shared ECS resource).

use crate::ecs::resource::Resource;

use super::types::AnimationClip;

/// Shared registry of loaded animation clips, indexed by handle.
pub struct AnimationClipRegistry {
    clips: Vec<AnimationClip>,
}

impl Resource for AnimationClipRegistry {}

impl AnimationClipRegistry {
    pub fn new() -> Self {
        Self { clips: Vec::new() }
    }

    pub fn add(&mut self, clip: AnimationClip) -> u32 {
        let handle = self.clips.len() as u32;
        self.clips.push(clip);
        handle
    }

    pub fn get(&self, handle: u32) -> Option<&AnimationClip> {
        self.clips.get(handle as usize)
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}

impl Default for AnimationClipRegistry {
    fn default() -> Self {
        Self::new()
    }
}
