//! Unit tests for the NIF→ECS import pipeline.
//!
//! Originally split out of `mod.rs` (#refactor) to keep the production
//! code under ~1000 lines; #2311 / TD1-083 finishes the job on this side,
//! which had grown back to 2030 LOC in a single file. The modules below
//! mirror the production import pipeline's concerns (transform/coord
//! composition, material/texture resolution, `BS*` subclass payloads,
//! particle systems, furniture markers, billboard mode), following the
//! same per-topic split `anim/tests/` already established.
//!
//! Pure code movement — every test body is byte-identical to its
//! pre-split form; only the module it lives in changed.

use super::*;
use crate::blocks::tri_shape::NiTriShapeData;
use crate::types::{BlockRef, NiPoint3};

mod app_culled_visibility;
mod billboard;
mod bs_subclass;
mod coord_cross_check;
mod core;
mod furniture;
mod material_texture;
mod particle;
mod transform;

/// Helper: build a minimal NifScene with the given blocks.
fn scene_from_blocks(blocks: Vec<Box<dyn crate::blocks::NiObject>>) -> NifScene {
    let root_index = if blocks.is_empty() { None } else { Some(0) };
    NifScene {
        blocks,
        root_index,
        ..NifScene::default()
    }
}

fn identity_transform() -> NiTransform {
    NiTransform::default()
}

fn translated(x: f32, y: f32, z: f32) -> NiTransform {
    NiTransform {
        translation: NiPoint3 { x, y, z },
        ..NiTransform::default()
    }
}

fn make_tri_shape_data() -> NiTriShapeData {
    NiTriShapeData {
        vertices: vec![
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            NiPoint3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            NiPoint3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        normals: vec![
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
        ],
        center: NiPoint3 {
            x: 0.33,
            y: 0.33,
            z: 0.0,
        },
        radius: 1.0,
        vertex_colors: Vec::new(),
        uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
        triangles: vec![[0, 1, 2]],
    }
}

fn make_ni_node(transform: NiTransform, children: Vec<BlockRef>) -> crate::blocks::node::NiNode {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("TestNode")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform,
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children,
        effects: Vec::new(),
    }
}

fn make_ni_tri_shape(
    name: &str,
    transform: NiTransform,
    data_ref: u32,
    properties: Vec<BlockRef>,
) -> crate::blocks::tri_shape::NiTriShape {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    crate::blocks::tri_shape::NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from(name)),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform,
            properties,
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef(data_ref),
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}
