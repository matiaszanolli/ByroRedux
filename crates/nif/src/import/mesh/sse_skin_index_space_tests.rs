//! Cross-check the two independently stored skin-weight channels in SSE assets.
use super::*;
use crate::blocks::skin::{BsDismemberSkinInstance, NiSkinInstance, NiSkinPartition};
use crate::blocks::tri_shape::BsTriShape;

#[test]
#[ignore = "requires installed Skyrim SE mesh archives"]
fn packed_sse_indices_match_partition_palette_expansion_on_real_data() {
    let override_data = std::env::var_os("BYROREDUX_SKYRIMSE_DATA");
    let explicit_override = override_data.is_some();
    let data = override_data
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data".into()
        });
    if !data.is_dir() {
        assert!(
            !explicit_override
                && !std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0"),
            "required Skyrim data missing: {}",
            data.display()
        );
        eprintln!("SKIP: Skyrim data missing: {}", data.display());
        return;
    }
    let archives: Vec<_> = ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"]
        .iter()
        .map(|name| byroredux_bsa::BsaArchive::open(data.join(name)).expect("open mesh archive"))
        .collect();
    let mut checked = 0;
    let mut changed = 0;
    for path in [
        r"meshes\actors\draugr\character assets\draugrmale.nif",
        r"meshes\actors\draugr\character assets\draugrfemale.nif",
        r"meshes\actors\character\character assets\malebody_1.nif",
        r"meshes\actors\character\character assets\malehands_1.nif",
        r"meshes\actors\character\facegendata\facegeom\skyrim.esm\00067667.nif",
    ] {
        let before = checked;
        let bytes = archives
            .iter()
            .find_map(|archive| archive.extract(path).ok())
            .unwrap_or_else(|| panic!("missing mesh {path}"));
        let scene = crate::parse_nif(&bytes).expect("parse NIF");
        for block in &scene.blocks {
            let Some(shape) = block.as_any().downcast_ref::<BsTriShape>() else {
                continue;
            };
            let Some(skin_idx) = shape.skin_ref.index() else {
                continue;
            };
            let inst = scene
                .get_as::<NiSkinInstance>(skin_idx)
                .or_else(|| {
                    scene
                        .get_as::<BsDismemberSkinInstance>(skin_idx)
                        .map(|i| &i.base)
                })
                .expect("legacy skin instance");
            let part = scene
                .get_as::<NiSkinPartition>(inst.skin_partition_ref.index().unwrap())
                .unwrap();
            let (weights, raw) = decode_sse_skin_payload(&scene, shape).expect("packed SSE skin");
            let imported = extract_skin_bs_tri_shape(&scene, shape, &[]).expect("import skin");
            for partition in &part.partitions {
                let stride = partition.num_weights_per_vertex as usize;
                for (local, &global) in partition.vertex_map.iter().enumerate() {
                    for lane in 0..stride.min(4) {
                        let pslot = local * stride + lane;
                        let weight = partition.vertex_weights[pslot];
                        if weight < 0.001 {
                            continue;
                        }
                        let expected = partition.bones[partition.bone_indices[pslot] as usize];
                        let vertex = global as usize;
                        assert!(
                            (weights[vertex][lane] - weight).abs() < 0.002,
                            "weight channel drift {path}"
                        );
                        assert_eq!(
                            raw[vertex][lane] as u16, expected,
                            "packed/partition disagreement {path} vertex={vertex} lane={lane}"
                        );
                        checked += 1;
                        changed +=
                            usize::from(imported.vertex_bone_indices[vertex][lane] != expected);
                    }
                }
            }
        }
        assert!(checked > before, "no weighted lanes checked for {path}");
    }
    eprintln!("SSE weighted lanes checked={checked}, importer incorrectly changed={changed}");
    assert!(checked > 1000, "probe must exercise real skin data");
    assert_eq!(
        changed, 0,
        "packed indices are already skin-global, not partition-local"
    );
}
