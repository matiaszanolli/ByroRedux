//! QUST alias-block raw-dump cross-validator: dump every raw sub-record
//! of a single `QUST` record by FormID, so an unexpected/unimplemented
//! sub-type inside an alias block can be spotted directly (companion to
//! `qust_alias_survey`, which only sees what the parser already decodes).
//!
//! This is the standing "does the wiki table match reality" tool cited by
//! `docs/engine/m47-3-quest-alias-design.md` — reuse it whenever the
//! alias sub-record table is extended, before trusting the synthetic
//! fixtures alone.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example qust_alias_rawdump -- <ESM> <hex form_id>

use byroredux_plugin::esm::reader::EsmReader;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .expect("usage: qust_alias_rawdump <ESM> <hex form_id>");
    let target = u32::from_str_radix(
        args.next()
            .expect("need a hex form_id")
            .trim_start_matches("0x"),
        16,
    )?;

    let bytes = std::fs::read(&esm_path)?;
    let mut reader = EsmReader::new(&bytes);
    // Only the first matching QUST is dumped — the form ID is unique in a
    // single plugin, so the guard just preserves the old stop-at-first hit.
    let mut found = false;
    reader.for_each_record(bytes.len(), Some(b"QUST"), |header, subs| {
        if found || header.form_id != target {
            return;
        }
        found = true;
        println!("QUST {:08X} — {} sub-records", header.form_id, subs.len());
        for s in &subs {
            let name = std::str::from_utf8(&s.sub_type).unwrap_or("????");
            let hex: Vec<String> = s.data.iter().take(24).map(|b| format!("{b:02x}")).collect();
            println!("  {name:>4}  len={:<4}  {}", s.data.len(), hex.join(" "));
        }
    })
}
