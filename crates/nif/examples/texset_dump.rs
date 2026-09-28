//! Scratch: print every BSShaderTextureSet's slots for a NIF.
use byroredux_nif::blocks::shader::BSShaderTextureSet;

fn main() {
    let path = std::env::args().nth(1).expect("usage: texset_dump <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    for idx in 0..scene.len() {
        if let Some(set) = scene.get_as::<BSShaderTextureSet>(idx) {
            for (slot, tex) in set.textures.iter().enumerate() {
                println!("block {idx} slot {slot}: {tex:?}");
            }
        }
    }
}
