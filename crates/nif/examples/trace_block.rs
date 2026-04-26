//! trace_block — dump per-block position + type for a single NIF.
//!
//! Usage:
//!   cargo run -p byroredux-nif --example trace_block -- <path>
//!   cargo run -p byroredux-nif --example trace_block -- <bsa> <path-in-bsa>
//!
//! Useful for bisecting parser misalignment on Oblivion files that
//! fail with "failed to fill whole buffer" — the output gives the
//! exact block start position, name, and a short hex dump of the
//! first 32 bytes so you can hand-decode the next field.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::{blocks::parse_block, header::NifHeader, stream::NifStream};
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    let data = match args.len() {
        2 => std::fs::read(&args[1]).expect("read file"),
        3 => {
            // Try BSA first; fall back to BA2 (FO4+).
            if let Ok(archive) = BsaArchive::open(&args[1]) {
                archive.extract(&args[2]).expect("extract NIF (BSA)")
            } else {
                let archive = Ba2Archive::open(&args[1]).expect("open BA2");
                archive.extract(&args[2]).expect("extract NIF (BA2)")
            }
        }
        _ => {
            eprintln!("usage: trace_block <path> | <archive> <path-in-archive>");
            std::process::exit(1);
        }
    };

    let (header, block_data_offset) = NifHeader::parse(&data).expect("parse header");
    println!("version: {}", header.version);
    println!("user_version: {}", header.user_version);
    println!("user_version_2 (bsver): {}", header.user_version_2);
    println!("num_blocks: {}", header.num_blocks);
    println!(
        "block_types: {}  block_sizes: {}",
        header.block_types.len(),
        header.block_sizes.len(),
    );
    println!("block_data_offset: {}", block_data_offset);

    let block_bytes = &data[block_data_offset..];
    let mut stream = NifStream::new(block_bytes, &header);

    for i in 0..header.num_blocks as usize {
        let type_name = header.block_type_name(i).expect("block name").to_string();
        let block_size = header.block_sizes.get(i).copied();
        let start = stream.position();

        // Peek up to the block's full size for context (capped at 256
        // bytes so the line stays readable on big blocks). For failing
        // BSMeshLODTriShape / BSTriShape blocks the entire wire layout
        // is < 200 bytes, so this prints the full contents.
        let tail = &block_bytes[start as usize..];
        let peek_len = block_size
            .map(|s| (s as usize).min(256).min(tail.len()))
            .unwrap_or(tail.len().min(64));
        let peek = &tail[..peek_len];
        let hex: String = peek
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        print!(
            "[{:3}] @ {:>6}  {:<32}  size={:>6}  peek: {}",
            i,
            start,
            type_name,
            block_size
                .map(|s| s.to_string())
                .unwrap_or_else(|| "?".into()),
            hex,
        );

        match parse_block(&type_name, &mut stream, block_size) {
            Ok(_) => {
                let consumed = stream.position() - start;
                println!("  [consumed {}]", consumed);
            }
            Err(e) => {
                let consumed = stream.position() - start;
                println!("  [ERR at consumed {}: {}]", consumed, e);
                break;
            }
        }
    }
}
