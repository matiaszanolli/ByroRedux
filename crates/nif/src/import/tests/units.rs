use super::*;
use crate::blocks::bs_geometry::{
    BSGeometry, BSGeometryMesh, BSGeometryMeshData, BSGeometryMeshKind,
};
use byroredux_core::math::{Mat4, Vec3};

#[test]
fn starfield_flat_and_hierarchical_imports_use_the_same_world_units() {
    // Native mesh body positions are metric, but the raw decoder exposes
    // nifly tooling units. The public importer must resolve BOTH that scale
    // and the raw metric node transforms, not scale only vertices or roots.
    let tooling = BSGeometryMeshData::HAVOK_SCALE;
    let data = BSGeometryMeshData {
        version: 2,
        triangles: vec![[0, 1, 2]],
        scale: 1.0,
        weights_per_vert: 0,
        vertices: vec![[0.0; 3], [tooling, 0.0, 0.0], [0.0, tooling, 0.0]],
        uvs0: vec![],
        uvs1: vec![],
        colors: vec![],
        // Source +Z normal and +X tangent, with positive handedness.
        normals_raw: vec![512 | (512 << 10) | (1023 << 20); 3],
        tangents_raw: vec![1023 | (512 << 10) | (512 << 20) | (3 << 30); 3],
        skin_weights: vec![],
        lods: vec![],
        meshlets: vec![],
        cull_data: vec![],
    };
    let mut av = make_ni_node(translated(0.0, 0.0, 0.5), vec![]).av;
    av.flags = 0x200;
    let shape = BSGeometry {
        av,
        bounding_sphere: ([0.0; 3], 1.0),
        bound_min_max: [0.0; 6],
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        meshes: vec![BSGeometryMesh {
            lod_slot: 0,
            tri_size: 3,
            num_verts: 3,
            flags: 0,
            kind: BSGeometryMeshKind::Internal {
                mesh_data: Box::new(data),
            },
        }],
    };
    let mut scene = scene_from_blocks(vec![
        Box::new(make_ni_node(translated(2.0, 0.0, 0.0), vec![BlockRef(1)])),
        Box::new(make_ni_node(translated(0.0, 0.0, 1.0), vec![BlockRef(2)])),
        Box::new(shape),
    ]);
    scene.bsver = crate::version::bsver::STARFIELD;
    let mut pool = StringPool::new();
    let flat = import_nif(&scene, &mut pool);
    let (collision_flat, _) = import_nif_with_collision(&scene, &mut pool);
    let tree = import_nif_scene(&scene, &mut pool);
    assert_eq!(flat.len(), 1);
    assert_eq!(tree.meshes.len(), 1);
    assert_eq!(tree.nodes.len(), 2);
    for mesh in [&flat[0], &collision_flat[0], &tree.meshes[0]] {
        assert!((mesh.positions[1][0] - 70.0).abs() < 0.0001);
        assert!((mesh.local_bound_radius - 70.0).abs() < 0.0001);
        assert_eq!(mesh.scale, 1.0);
        assert!((mesh.positions[2][2] + 70.0).abs() < 0.0001);
        assert_eq!(mesh.positions[2][1], 0.0);
        assert!(mesh.normals[0][1] > 0.999);
        assert!(mesh.normals[0][2].abs() < 0.002);
        assert!(mesh.tangents[0][0] > 0.999);
        assert_eq!(mesh.tangents[0][3], 1.0);
    }
    assert_eq!(flat[0].translation, [140.0, 105.0, -0.0]);
    let sum = tree
        .nodes
        .iter()
        .fold(Vec3::ZERO, |p, n| p + Vec3::from_array(n.translation))
        + Vec3::from_array(tree.meshes[0].translation);
    assert_eq!(sum.to_array(), flat[0].translation);
}

#[test]
fn starfield_skin_changes_units_without_scaling_the_linear_map() {
    let scene = NifScene {
        bsver: crate::version::bsver::STARFIELD,
        ..Default::default()
    };
    let mut mesh = ImportedMesh::from_geometry(
        vec![[0.0, BSGeometryMeshData::HAVOK_SCALE, 0.0]],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    mesh.bs_geometry_lod_slot = Some(0);
    mesh.skin = Some(ImportedSkin {
        bones: vec![ImportedBone {
            name: Arc::from("joint"),
            bind_inverse: Mat4::from_translation(-Vec3::Y).to_cols_array_2d(),
            bounding_sphere: [0.0, 1.0, 0.0, 0.25],
        }],
        ..Default::default()
    });
    super::super::units::meshes(&scene, std::slice::from_mut(&mut mesh));
    let bone = &mesh.skin.as_ref().unwrap().bones[0];
    assert_eq!(bone.bounding_sphere, [0.0, 70.0, 0.0, 17.5]);
    let bind = Mat4::from_cols_array_2d(&bone.bind_inverse);
    let posed = Mat4::from_translation(Vec3::Y * 70.0)
        * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)
        * bind;
    assert!(
        (posed.transform_point3(Vec3::from_array(mesh.positions[0])) - Vec3::Y * 70.0).length()
            < 0.0001
    );
    assert_eq!(bind.x_axis.x, 1.0);
}

#[test]
fn legacy_import_spatial_values_are_bit_identical() {
    for bsver in [0, 34, 83, 100, 130, 155] {
        let scene = NifScene {
            bsver,
            ..Default::default()
        };
        let mut mesh = ImportedMesh::from_geometry(
            vec![[1.25, -0.0, 3.5]],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        mesh.translation = [2.0, -0.0, 4.0];
        super::super::units::meshes(&scene, std::slice::from_mut(&mut mesh));
        assert_eq!(
            mesh.positions[0].map(f32::to_bits),
            [1.25f32, -0.0, 3.5].map(f32::to_bits)
        );
        assert_eq!(
            mesh.translation.map(f32::to_bits),
            [2.0f32, -0.0, 4.0].map(f32::to_bits)
        );
    }
}

#[test]
fn starfield_nif_light_positions_and_attenuation_ranges_share_units() {
    use crate::blocks::light::{NiLightBase, NiPointLight};
    use crate::types::NiColor;
    let color = NiColor {
        r: 1.0,
        g: 0.5,
        b: 0.25,
    };
    let lamp = NiPointLight {
        base: NiLightBase {
            av: make_ni_node(translated(0.0, 0.0, 3.0), vec![]).av,
            switch_state: true,
            affected_nodes: vec![],
            dimmer: 0.75,
            ambient_color: color,
            diffuse_color: color,
            specular_color: color,
        },
        constant_attenuation: 0.0,
        linear_attenuation: 0.0,
        quadratic_attenuation: 1.0,
    };
    let mut scene = scene_from_blocks(vec![
        Box::new(make_ni_node(translated(2.0, 0.0, 0.0), vec![BlockRef(1)])),
        Box::new(lamp),
    ]);
    let legacy = import_nif_lights(&scene).remove(0);
    scene.bsver = crate::version::bsver::STARFIELD;
    let metric = import_nif_lights(&scene).remove(0);
    assert_eq!(metric.translation, [140.0, 210.0, -0.0]);
    assert_eq!(metric.radius, legacy.radius * 70.0);
    assert_eq!(metric.color, legacy.color);
    assert_eq!(metric.direction, legacy.direction);
}

#[test]
fn starfield_animation_scales_distances_but_not_timing_or_dimensionless_keys() {
    use crate::anim::*;
    use crate::blocks::interpolator::KeyType;
    let key = TranslationKey {
        time: 0.25,
        value: [1.0, 2.0, 3.0],
        forward: [0.5; 3],
        backward: [-0.5; 3],
        tbc: Some([0.1, 0.2, 0.3]),
    };
    let channel = TransformChannel {
        translation_keys: vec![key],
        translation_type: KeyType::Quadratic,
        rotation_keys: vec![],
        rotation_type: KeyType::Linear,
        scale_keys: vec![ScaleKey {
            time: 0.25,
            value: 2.0,
            forward: 0.5,
            backward: -0.5,
            tbc: None,
        }],
        scale_type: KeyType::Quadratic,
        priority: 0,
    };
    let mut clip = AnimationClip {
        name: "metric".into(),
        duration: 1.0,
        cycle_type: CycleType::Loop,
        frequency: 2.0,
        phase: 0.25,
        weight: 0.75,
        accum_root_name: None,
        channels: [(Arc::from("joint"), channel)].into_iter().collect(),
        float_channels: [FloatTarget::LightRadius, FloatTarget::LightDimmer]
            .into_iter()
            .map(|target| {
                (
                    Arc::from("lamp"),
                    FloatChannel {
                        target,
                        keys: vec![AnimFloatKey {
                            time: 0.25,
                            value: 2.0,
                        }],
                    },
                )
            })
            .collect(),
        color_channels: vec![],
        bool_channels: vec![],
        texture_flip_channels: vec![],
        text_keys: vec![],
    };
    let scene = NifScene {
        bsver: crate::version::bsver::STARFIELD,
        ..Default::default()
    };
    super::super::units::animation(&scene, &mut clip);
    let channel = &clip.channels["joint"];
    let normalized = channel.translation_keys[0];
    assert_eq!(normalized.value, [70.0, 140.0, 210.0]);
    assert_eq!(normalized.forward, [35.0; 3]);
    assert_eq!(normalized.backward, [-35.0; 3]);
    assert_eq!(normalized.time, key.time);
    assert_eq!(normalized.tbc, key.tbc);
    assert_eq!(channel.scale_keys[0].value, 2.0);
    assert_eq!(clip.float_channels[0].1.keys[0].value, 140.0);
    assert_eq!(clip.float_channels[1].1.keys[0].value, 2.0);
    assert_eq!(
        (clip.duration, clip.frequency, clip.phase, clip.weight),
        (1.0, 2.0, 0.25, 0.75)
    );
}
