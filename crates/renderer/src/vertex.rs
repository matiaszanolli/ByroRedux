//! Vertex format definition.

use ash::vk;

/// Per-vertex data for position + color rendering.
///
/// `#[repr(C)]` ensures the layout matches what the shader expects.
/// Will be extended with UVs in Phase 5 (texturing).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub const fn new(position: [f32; 3], color: [f32; 3]) -> Self {
        Self { position, color }
    }

    /// How vertex data is read from the buffer (stride, rate).
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Self>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    /// Per-attribute layout within a vertex.
    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 2] {
        [
            // location 0: position (vec3)
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            // location 1: color (vec3)
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: std::mem::size_of::<[f32; 3]>() as u32,
            },
        ]
    }
}
