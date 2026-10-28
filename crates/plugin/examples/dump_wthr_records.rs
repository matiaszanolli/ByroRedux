//! Dump every decoded `WeatherRecord` of an ESM, sorted by FormID, as `Debug` text.
//!
//! A refactor pin for `parse_wthr` / `parse_wthr_creation` (#6223): dump a master before and
//! after the change and `diff` the two files — a behaviour-preserving refactor leaves them
//! byte-identical, which the hand-built per-game fixtures cannot show for real content.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example dump_wthr_records -- \
//!       <oblivion|fo3nv|skyrim|fo4|fo76|starfield> <ESM> > out.txt

use byroredux_plugin::esm::reader::{EsmReader, GameKind};
use byroredux_plugin::esm::records::parse_wthr;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(game), Some(path)) = (args.next(), args.next()) else {
        anyhow::bail!("usage: <oblivion|fo3nv|skyrim|fo4|fo76|starfield> <ESM>");
    };
    let game = match game.as_str() {
        "oblivion" => GameKind::Oblivion,
        "fo3nv" => GameKind::Fallout3NV,
        "skyrim" => GameKind::Skyrim,
        "fo4" => GameKind::Fallout4,
        "fo76" => GameKind::Fallout76,
        "starfield" => GameKind::Starfield,
        other => anyhow::bail!("unknown game {other:?}"),
    };
    let bytes = std::fs::read(&path)?;
    let mut reader = EsmReader::new(&bytes);
    reader.read_file_header()?;
    let mut records = Vec::new();
    reader.for_each_record(bytes.len(), Some(b"WTHR"), |header, subs| {
        records.push(parse_wthr(header.form_id, &subs, game, &None));
    })?;
    records.sort_by_key(|record| record.form_id);
    println!("# {} WTHR records from {path}", records.len());
    for record in &records {
        println!("{record:#?}");
    }
    Ok(())
}
