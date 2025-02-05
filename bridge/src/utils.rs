use std::{
    fs::{create_dir_all, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use bitcoin::Network;
use bitcoin_script::{script, Script};
use bitvm::{bigint::BigIntImpl, pseudo::NMUL};
use serde::{Deserialize, Serialize};

const NUM_BLOCKS_REGTEST: u32 = 2;
const NUM_BLOCKS_TESTNET: u32 = 2;

pub fn num_blocks_per_network(network: Network, mainnet_num_blocks: u32) -> u32 {
    match network {
        Network::Bitcoin => mainnet_num_blocks,
        Network::Regtest => NUM_BLOCKS_REGTEST,
        _ => NUM_BLOCKS_TESTNET, // Testnet, Signet
    }
}

pub fn remove_script_and_control_block_from_witness(mut witness: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    witness.truncate(witness.len() - 2);

    witness
}

// Source for below hash handling functions in script:
// https://github.com/alpenlabs/strata-bridge-poc/tree/main/crates/primitives/src/scripts/transform.rs
const LIMB_SIZE: u32 = 30;
pub type H256 = BigIntImpl<256, LIMB_SIZE>;

fn split_digit(window: u32, index: u32) -> Script {
    script! {
        // {v}
        0                           // {v} {A}
        OP_SWAP
        for i in 0..index {
            OP_TUCK                 // {v} {A} {v}
            { 1 << (window - i - 1) }   // {v} {A} {v} {1000}
            OP_GREATERTHANOREQUAL   // {v} {A} {1/0}
            OP_TUCK                 // {v} {1/0} {A} {1/0}
            OP_ADD                  // {v} {1/0} {A+1/0}
            if i < index - 1 { { NMUL(2) } }
            OP_ROT OP_ROT
            OP_IF
                { 1 << (window - i - 1) }
                OP_SUB
            OP_ENDIF
        }
        OP_SWAP
    }
}

pub fn sb_hash_from_nibbles() -> Script {
    const WINDOW: u32 = 4;
    const N_DIGITS: u32 = (H256::N_BITS + WINDOW - 1) / WINDOW;

    script! {
        for i in 1..64 { { i } OP_ROLL }
        for i in (1..=N_DIGITS).rev() {
            if (i * WINDOW) % LIMB_SIZE == 0 {
                OP_TOALTSTACK
            } else if (i * WINDOW) % LIMB_SIZE > 0 &&
                        (i * WINDOW) % LIMB_SIZE < WINDOW {
                OP_SWAP
                { split_digit(WINDOW, (i * WINDOW) % LIMB_SIZE) }
                OP_ROT
                { NMUL(1 << ((i * WINDOW) % LIMB_SIZE)) }
                OP_ADD
                OP_TOALTSTACK
            } else if i != N_DIGITS {
                { NMUL(1 << WINDOW) }
                OP_ADD
            }
        }
        for _ in 1..H256::N_LIMBS { OP_FROMALTSTACK }
        for i in 1..H256::N_LIMBS { { i } OP_ROLL }
    }
}

pub fn sb_hash_from_bytes() -> Script {
    const WINDOW: u32 = 8;
    const N_DIGITS: u32 = (H256::N_BITS + WINDOW - 1) / WINDOW;

    script! {
        for i in 1..32 { { i } OP_ROLL }
        for i in (1..=N_DIGITS).rev() {
            if (i * WINDOW) % LIMB_SIZE == 0 {
                OP_TOALTSTACK
            } else if (i * WINDOW) % LIMB_SIZE > 0 &&
                        (i * WINDOW) % LIMB_SIZE < WINDOW {
                OP_SWAP
                { split_digit(WINDOW, (i * WINDOW) % LIMB_SIZE) }
                OP_ROT
                { NMUL(1 << ((i * WINDOW) % LIMB_SIZE)) }
                OP_ADD
                OP_TOALTSTACK
            } else if i != N_DIGITS {
                { NMUL(1 << WINDOW) }
                OP_ADD
            }
        }
        for _ in 1..H256::N_LIMBS { OP_FROMALTSTACK }
        for i in 1..H256::N_LIMBS { { i } OP_ROLL }
    }
}

pub fn write_cache(file_path: &Path, data: &impl Serialize) -> std::io::Result<()> {
    println!("Writing cache to {}...", file_path.display());
    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            create_dir_all(parent)?;
        }
    }
    let file = File::create(file_path)?;

    let writer = BufWriter::new(file);

    let raw_data = serde_json::to_vec(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut compressor = brotli::CompressorWriter::new(writer, 4096, 5, 22);

    compressor.write_all(&raw_data)?;
    compressor.flush()?;

    Ok(())
}

pub fn read_cache<T>(file_path: &Path) -> std::io::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    println!("Reading cache from {}...", file_path.display());
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);

    let mut decompressed_data = Vec::new();

    let mut decompressor = brotli::Decompressor::new(&mut reader, 4096);

    decompressor.read_to_end(&mut decompressed_data)?;

    let deserialized: T = serde_json::from_slice(&decompressed_data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(deserialized)
}

pub fn cleanup_cache_files(prefix: &str, cache_location: &PathBuf, max_cache_files: u32) {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(cache_location)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().unwrap_or("").starts_with(prefix))
        .map(|entry| entry.path())
        .collect();

    paths.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| std::time::SystemTime::now())
    });

    if paths.len() >= max_cache_files as usize {
        if let Some(oldest) = paths.first() {
            std::fs::remove_file(oldest).expect("Failed to delete the oldest cache file");
            println!("Deleted oldest cache file: {:?}", oldest);
        }
    }
}
