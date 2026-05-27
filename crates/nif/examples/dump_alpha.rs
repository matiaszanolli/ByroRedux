//! Scratch: dump every NiAlphaProperty's blend flags in a NIF.
//! F-godray investigation (2026-05-27) — verify what blend mode FO4
//! light-shaft effect meshes author.

use byroredux_nif::blocks::properties::NiAlphaProperty;

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_alpha <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    for (i, block) in scene.blocks.iter().enumerate() {
        if let Some(a) = block.as_any().downcast_ref::<NiAlphaProperty>() {
            let flags = a.flags;
            let blend_enabled = flags & 0x0001 != 0;
            let src = (flags >> 1) & 0xF;
            let dst = (flags >> 5) & 0xF;
            let test_enabled = flags & 0x0200 != 0;
            // Gamebryo AlphaFunction enum: 0=ONE 1=ZERO 2=SRC_COLOR
            // 3=INV_SRC_COLOR 4=DEST_COLOR 5=INV_DEST_COLOR 6=SRC_ALPHA
            // 7=INV_SRC_ALPHA 8=DEST_ALPHA 9=INV_DEST_ALPHA 10=SRC_ALPHA_SAT
            let name = |f: u16| match f {
                0 => "ONE",
                1 => "ZERO",
                2 => "SRC_COLOR",
                3 => "INV_SRC_COLOR",
                4 => "DEST_COLOR",
                5 => "INV_DEST_COLOR",
                6 => "SRC_ALPHA",
                7 => "INV_SRC_ALPHA",
                8 => "DEST_ALPHA",
                9 => "INV_DEST_ALPHA",
                10 => "SRC_ALPHA_SAT",
                _ => "?",
            };
            println!(
                "block {:3}: flags=0x{:04X} blend={} src={} dst={} test={}",
                i,
                flags,
                blend_enabled,
                name(src),
                name(dst),
                test_enabled
            );
        }
    }
}
