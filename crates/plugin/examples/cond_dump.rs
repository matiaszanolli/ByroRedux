//! TEMP: dump one PACK's parsed CTDA conditions.
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let target: u32 = u32::from_str_radix(std::env::args().nth(2).unwrap().trim_start_matches("0x"), 16).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let index = byroredux_plugin::esm::parse_esm(&bytes).unwrap();
    let pack = index.packages.get(&target).unwrap();
    println!("{} {} procedure {}", target, pack.editor_id, pack.procedure_type);
    for c in pack.conditions.iter() {
        println!(
            "  fn={} {} op={} value={} p1={:#x} run_on={}",
            c.function_index,
            "?",
            format!("{:?}", c.comparator),
            format!("{:?}", c.comparand),
            c.param_1,
            format!("{:?}", c.run_on),
        );
    }
}
