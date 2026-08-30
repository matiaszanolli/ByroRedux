//! TEMP scratch (audit 2026-08-30): why does sequence_emitter_rate (#3329
//! tier d) recover nothing on FO3's 20 manager-blend emitter meshes?
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::controller::{ControlledBlock, NiControllerSequence};
use byroredux_nif::blocks::interpolator::{
    NiBlendFloatInterpolator, NiFloatData, NiFloatInterpolator,
};
use byroredux_nif::blocks::properties::NiStringPalette;
use byroredux_nif::parse_nif;
use byroredux_nif::scene::NifScene;

fn cb_ctype(scene: &NifScene, cb: &ControlledBlock) -> Option<String> {
    if let Some(s) = cb.controller_type.as_ref() {
        return Some(s.to_string());
    }
    let pal = scene.get_as::<NiStringPalette>(cb.string_palette_ref.index()?)?;
    pal.get_string(cb.controller_type_offset)
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    let root = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data";
    let arc = BsaArchive::open(&format!("{root}/Fallout - Meshes.bsa")).unwrap();
    let targets = [
        "meshes\\effects\\ambient\\fxbubblestall01.nif",
        "meshes\\architecture\\urban\\tenpengate01.nif",
        "meshes\\effects\\ambient\\fxfallingrocks01.nif",
    ];
    for t in targets {
        let Ok(bytes) = arc.extract(t) else {
            println!("{t}: MISSING");
            continue;
        };
        let scene = parse_nif(&bytes).unwrap();
        println!("== {t}");
        for (bi, b) in scene.blocks.iter().enumerate() {
            let Some(seq) = b.as_any().downcast_ref::<NiControllerSequence>() else {
                continue;
            };
            println!(
                "  seq[{bi}] name={:?} controlled_blocks={}",
                seq.name.as_deref(),
                seq.controlled_blocks.len()
            );
            for (ci, cb) in seq.controlled_blocks.iter().enumerate() {
                let ct = cb_ctype(&scene, cb);
                if ct.as_deref().map(|s| s.contains("Emitter")) != Some(true) {
                    continue;
                }
                let ii = cb.interpolator_ref.index();
                let ty = ii
                    .and_then(|i| scene.blocks.get(i))
                    .map(|b| b.block_type_name().to_string());
                let fv = ii.and_then(|i| scene.get_as::<NiFloatInterpolator>(i));
                let keys = fv
                    .and_then(|f| f.data_ref.index())
                    .and_then(|d| scene.get_as::<NiFloatData>(d))
                    .map(|d| {
                        d.keys
                            .keys
                            .iter()
                            .take(4)
                            .map(|k| k.value)
                            .collect::<Vec<_>>()
                    });
                let bl = ii
                    .and_then(|i| scene.get_as::<NiBlendFloatInterpolator>(i))
                    .map(|b| (b.value, b.base.items.len()));
                println!(
                    "    cb[{ci}] ctype={ct:?} interp={ty:?} float_value={:?} keys={keys:?} blend={bl:?}",
                    fv.map(|f| f.value)
                );
            }
        }
    }
}
