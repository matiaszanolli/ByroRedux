//! Tests for `pkin_expansion_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`pkin_expansion_tests::FOO`).

    //! Regression tests for #589 (FO4-DIM4-03) — PKIN (Pack-In) REFR
    //! expansion. Every `CNAM` content ref spawns at the outer REFR's
    //! transform. Pre-fix the 872 vanilla Fallout4.esm PKIN records
    //! were routed through the MODL-only parser and their CNAM lists
    //! were silently dropped.
    use super::*;
    use byroredux_plugin::esm::cell::EsmCellIndex;
    use byroredux_plugin::esm::records::PkinRecord;

    fn mk_pkin(form_id: u32, editor_id: &str, contents: Vec<u32>) -> PkinRecord {
        PkinRecord {
            form_id,
            editor_id: editor_id.to_string(),
            full_name: String::new(),
            contents,
            vnam_form_id: 0,
            flags: 0,
        }
    }

    /// Baseline: a base form that isn't a PKIN returns `None` so the
    /// caller falls through to the SCOL / default chain.
    #[test]
    fn expand_non_pkin_returns_none() {
        let index = EsmCellIndex::default();
        let result = expand_pkin_placements(
            0x0010_ABCD,
            Vec3::new(100.0, 50.0, -25.0),
            Quat::IDENTITY,
            2.0,
            &index,
        );
        assert!(result.is_none());
    }

    /// Vanilla shape: a PKIN with a single CNAM fans out to one
    /// synthetic placement at the outer transform. This is how FO4
    /// workbench-loot bundles render end-to-end.
    #[test]
    fn expand_pkin_single_cnam_fans_out_to_one_synth() {
        let mut index = EsmCellIndex::default();
        let pkin_id = 0x0055_0001;
        index
            .packins
            .insert(pkin_id, mk_pkin(pkin_id, "WorkbenchLoot", vec![0x0020_0001]));

        let outer_pos = Vec3::new(500.0, 100.0, 250.0);
        let outer_rot = Quat::IDENTITY;
        let outer_scale = 1.5;
        let synths = expand_pkin_placements(pkin_id, outer_pos, outer_rot, outer_scale, &index)
            .expect("PKIN with a CNAM must fan out");
        assert_eq!(synths.len(), 1);
        assert_eq!(synths[0].0, 0x0020_0001);
        assert_eq!(synths[0].1, outer_pos);
        assert_eq!(synths[0].2, outer_rot);
        // Outer scale propagates verbatim — PKIN has no per-child scale.
        assert_eq!(synths[0].3, outer_scale);
    }

    /// Multi-CNAM bundle: each content ref becomes a synth. All synths
    /// share the outer transform; authoring order is preserved so
    /// downstream consumers iterate in the right sequence.
    #[test]
    fn expand_pkin_multiple_cnam_preserves_order_at_outer_transform() {
        let mut index = EsmCellIndex::default();
        let pkin_id = 0x0055_0002;
        index.packins.insert(
            pkin_id,
            mk_pkin(
                pkin_id,
                "MultiPack",
                vec![0x0020_0001, 0x0020_0002, 0x0020_0003],
            ),
        );

        let outer_pos = Vec3::new(10.0, 20.0, 30.0);
        let outer_rot = Quat::from_rotation_y(0.5);
        let outer_scale = 1.0;
        let synths =
            expand_pkin_placements(pkin_id, outer_pos, outer_rot, outer_scale, &index).unwrap();
        assert_eq!(synths.len(), 3);
        assert_eq!(synths[0].0, 0x0020_0001);
        assert_eq!(synths[1].0, 0x0020_0002);
        assert_eq!(synths[2].0, 0x0020_0003);
        // Every synth shares the outer transform exactly.
        for s in &synths {
            assert_eq!(s.1, outer_pos);
            assert_eq!(s.2, outer_rot);
            assert_eq!(s.3, outer_scale);
        }
    }

    /// A PKIN with an empty `contents` list (malformed / author-trimmed)
    /// returns `None` so the caller falls through rather than spawning
    /// zero synthetic children at the outer transform. Prevents the
    /// outer REFR from being silently dropped — the single-entry
    /// default path still runs its own stat lookup.
    #[test]
    fn expand_pkin_with_empty_contents_returns_none() {
        let mut index = EsmCellIndex::default();
        let pkin_id = 0x0055_0003;
        index
            .packins
            .insert(pkin_id, mk_pkin(pkin_id, "EmptyBundle", Vec::new()));
        let result = expand_pkin_placements(
            pkin_id,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            &index,
        );
        assert!(result.is_none());
    }
