//! Tests for `sky_params_cleanup_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`sky_params_cleanup_tests::FOO`).

    //! Regression coverage for #626 — `SkyParamsRes` texture handles
    //! must reach `unload_cell`'s drop list. The Vulkan-dependent half
    //! of `unload_cell` can't be unit-tested in isolation, so we cover
    //! the upstream contract: `SkyParamsRes::texture_indices` enumerates
    //! every bindless slot the struct owns. A future contributor who
    //! adds a 6th slot but forgets to extend `texture_indices` will see
    //! the count assertion fail; the comment on `texture_indices` then
    //! redirects them to update both sites.
    use super::*;

    fn mk_sky(indices: [u32; 5]) -> SkyParamsRes {
        SkyParamsRes {
            zenith_color: [0.0; 3],
            horizon_color: [0.0; 3],
            sun_direction: [0.0, 1.0, 0.0],
            sun_color: [1.0; 3],
            sun_size: 1.0,
            sun_intensity: 1.0,
            is_exterior: true,
            cloud_scroll: [0.0; 2],
            cloud_tile_scale: 1.0,
            cloud_texture_index: indices[0],
            sun_texture_index: indices[4],
            cloud_scroll_1: [0.0; 2],
            cloud_tile_scale_1: 1.0,
            cloud_texture_index_1: indices[1],
            cloud_scroll_2: [0.0; 2],
            cloud_tile_scale_2: 1.0,
            cloud_texture_index_2: indices[2],
            cloud_scroll_3: [0.0; 2],
            cloud_tile_scale_3: 1.0,
            cloud_texture_index_3: indices[3],
        }
    }

    /// Every distinct texture-index field on `SkyParamsRes` must be
    /// surfaced by `texture_indices`. Adding a new bindless slot
    /// without extending the helper would regress #626 (texture
    /// refcounts leaked across cell unloads).
    #[test]
    fn texture_indices_enumerates_all_five_slots() {
        let sky = mk_sky([10, 20, 30, 40, 50]);
        let mut indices: Vec<u32> = sky.texture_indices().into_iter().collect();
        indices.sort();
        assert_eq!(indices, vec![10, 20, 30, 40, 50]);
    }

    /// The `SkyParamsRes` insert path must round-trip through the
    /// resource API so `unload_cell`'s `try_resource` + `remove_resource`
    /// sequence sees the values written by `scene.rs`. Guards against a
    /// future World API change that would break the resource type
    /// dispatch for this resource specifically.
    #[test]
    fn sky_params_resource_round_trip() {
        let mut world = World::new();
        world.insert_resource(mk_sky([1, 2, 3, 4, 5]));
        {
            let sky = world.try_resource::<SkyParamsRes>().expect("present");
            let mut got: Vec<u32> = sky.texture_indices().into_iter().collect();
            got.sort();
            assert_eq!(got, vec![1, 2, 3, 4, 5]);
        }
        let removed = world
            .remove_resource::<SkyParamsRes>()
            .expect("remove returns Some");
        let mut got: Vec<u32> = removed.texture_indices().into_iter().collect();
        got.sort();
        assert_eq!(got, vec![1, 2, 3, 4, 5]);
        assert!(world.try_resource::<SkyParamsRes>().is_none());
    }
