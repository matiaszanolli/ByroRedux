//! Tests for `skin_tests` extracted from ../mesh.rs (refactor stage A).
//!
//! Same qualified path preserved (`skin_tests::FOO`).

    use super::*;
    use crate::blocks::skin::{BoneData, BoneVertWeight};
    use crate::types::NiMatrix3;

    fn identity_transform() -> NiTransform {
        NiTransform {
            rotation: NiMatrix3 {
                rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            },
            translation: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 1.0,
        }
    }

    fn bone(weights: Vec<(u16, f32)>) -> BoneData {
        BoneData {
            skin_transform: identity_transform(),
            bounding_sphere: [0.0, 0.0, 0.0, 0.0],
            vertex_weights: weights
                .into_iter()
                .map(|(vertex_index, weight)| BoneVertWeight {
                    vertex_index,
                    weight,
                })
                .collect(),
        }
    }

    #[test]
    fn densify_empty_data_gives_default_binding() {
        // No bones at all — every vertex should fall back to bone 0 weight 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: Vec::new(),
        };
        let (indices, weights) = densify_sparse_weights(3, &data);
        assert_eq!(indices.len(), 3);
        assert_eq!(weights.len(), 3);
        for i in 0..3 {
            assert_eq!(indices[i], [0, 0, 0, 0]);
            assert_eq!(weights[i], [1.0, 0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn densify_single_bone_full_weight() {
        // Bone 0 binds vertex 0 with weight 1.0, vertex 1 not bound.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![bone(vec![(0, 1.0)])],
        };
        let (indices, weights) = densify_sparse_weights(2, &data);
        assert_eq!(indices[0], [0, 0, 0, 0]);
        assert!((weights[0][0] - 1.0).abs() < 1e-6);
        // Vertex 1 falls back to bone 0 weight 1.
        assert_eq!(indices[1], [0, 0, 0, 0]);
        assert_eq!(weights[1], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn densify_two_bones_normalized() {
        // Vertex 0 gets half-and-half from bones 0 and 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![bone(vec![(0, 0.5)]), bone(vec![(0, 0.5)])],
        };
        let (indices, weights) = densify_sparse_weights(1, &data);
        // Two slots used, two unused. Weights sum to 1.
        let total: f32 = weights[0].iter().sum();
        assert!((total - 1.0).abs() < 1e-5);
        // Exactly two distinct bones present (0 and 1). Order inside
        // the 4-slot tuple isn't guaranteed by the algorithm.
        let mut seen: Vec<u16> = indices[0]
            .iter()
            .zip(weights[0].iter())
            .filter(|(_, w)| **w > 0.0)
            .map(|(b, _)| *b)
            .collect();
        seen.sort();
        assert_eq!(seen, vec![0u16, 1]);
    }

    #[test]
    fn densify_more_than_four_bones_keeps_top_four_by_weight() {
        // Five bones all bind vertex 0 with increasing weight. The top
        // 4 (weights 0.2, 0.3, 0.4, 0.5) should survive; the smallest
        // (0.1) should be dropped. After normalization the kept weights
        // sum to 1.
        let data = NiSkinData {
            skin_transform: identity_transform(),
            bones: vec![
                bone(vec![(0, 0.1)]), // bone 0 — should be dropped
                bone(vec![(0, 0.2)]),
                bone(vec![(0, 0.3)]),
                bone(vec![(0, 0.4)]),
                bone(vec![(0, 0.5)]),
            ],
        };
        let (indices, weights) = densify_sparse_weights(1, &data);

        let total: f32 = weights[0].iter().sum();
        assert!((total - 1.0).abs() < 1e-5, "weights should sum to 1");

        let mut present: Vec<(u16, f32)> = indices[0]
            .iter()
            .zip(weights[0].iter())
            .filter(|(_, w)| **w > 0.0)
            .map(|(b, w)| (*b, *w))
            .collect();
        assert_eq!(present.len(), 4, "should keep exactly 4 bones");
        present.sort_by_key(|(b, _)| *b);

        // Dropped bone 0 (weight 0.1); kept bones 1..=4.
        let bones: Vec<u16> = present.iter().map(|(b, _)| *b).collect();
        assert_eq!(bones, vec![1u16, 2, 3, 4]);

        // Original sum = 0.2 + 0.3 + 0.4 + 0.5 = 1.4; after normalizing
        // each weight becomes w / 1.4.
        assert!((present[0].1 - 0.2 / 1.4).abs() < 1e-5);
        assert!((present[3].1 - 0.5 / 1.4).abs() < 1e-5);
    }

    #[test]
    fn ni_transform_to_yup_matrix_identity() {
        let t = identity_transform();
        let m = ni_transform_to_yup_matrix(&t);
        // Identity rotation through C * I * C^T = I, identity translation, scale 1.
        // Column 0 = (1,0,0,0), col 1 = (0,1,0,0), col 2 = (0,0,1,0), col 3 = (0,0,0,1)
        assert!((m[0][0] - 1.0).abs() < 1e-6);
        assert!((m[1][1] - 1.0).abs() < 1e-6);
        assert!((m[2][2] - 1.0).abs() < 1e-6);
        assert!((m[3][3] - 1.0).abs() < 1e-6);
        // Off-diagonals zero.
        assert!(m[0][1].abs() < 1e-6);
        assert!(m[1][0].abs() < 1e-6);
    }

    #[test]
    fn ni_transform_to_yup_matrix_translation_only() {
        // Gamebryo Z-up translation (1, 2, 3) → Y-up (1, 3, -2).
        let t = NiTransform {
            rotation: NiMatrix3 {
                rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            },
            translation: NiPoint3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            scale: 1.0,
        };
        let m = ni_transform_to_yup_matrix(&t);
        // Column 3 holds the translation in column-major storage.
        assert!((m[3][0] - 1.0).abs() < 1e-6);
        assert!((m[3][1] - 3.0).abs() < 1e-6);
        assert!((m[3][2] + 2.0).abs() < 1e-6);
    }

    #[test]
    fn ni_transform_to_yup_matrix_scale_baked_in() {
        let mut t = identity_transform();
        t.scale = 2.5;
        let m = ni_transform_to_yup_matrix(&t);
        // Diagonal should be scale.
        assert!((m[0][0] - 2.5).abs() < 1e-6);
        assert!((m[1][1] - 2.5).abs() < 1e-6);
        assert!((m[2][2] - 2.5).abs() < 1e-6);
        // W column still identity.
        assert!((m[3][3] - 1.0).abs() < 1e-6);
    }
