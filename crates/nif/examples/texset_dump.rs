//! Scratch: run the full material import for a NIF and print each mesh's
//! resolved normal-map path, to chase the `textures/\bnor` missing report.
use byroredux_core::string::StringPool;

fn main() {
    let path = std::env::args().nth(1).expect("usage: texset_dump <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    let mut pool = StringPool::new();
    let imported = byroredux_nif::import::import_nif(&scene, &mut pool);
    for m in &imported {
        let material = &m.material;
        let show = |sym: &Option<byroredux_core::string::FixedString>| {
            sym.as_ref()
                .and_then(|sym| pool.resolve(*sym))
                .map(str::to_owned)
                .unwrap_or_else(|| "<none>".into())
        };
        println!(
            "{:?}  normal={}  base={}  src_normal={:?}",
            m.name,
            show(&material.textures.normal),
            show(&material.textures.base_color),
            material.texture_sources.normal,
        );
    }
}
