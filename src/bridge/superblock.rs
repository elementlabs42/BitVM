use std::mem::size_of;

use bitcoin::{block::Header, consensus::encode::serialize, hashes::Hash, BlockHash};
use bitcoin_script::{script, Script};

use crate::pseudo::NMUL;

/*
  TODO: Implement selecting a block that marks the start of a superblock measurement period
  that lasts for the period ∆C (e.g. 2000 blocks), during which the operator must observe
  all blocks on the main chain and identify the heaviest superblock SB.
*/
pub fn get_start_time_block_number() -> u32 { return 161249; }

pub fn find_superblock() -> Header { todo!() }

pub fn get_superblock_message(sb: &Header) -> Vec<u8> { serialize(sb) }

pub const SUPERBLOCK_MESSAGE_LENGTH: usize = size_of::<Header>();

pub fn get_superblock_hash_message(sb: &Header) -> Vec<u8> {
    sb.block_hash().as_byte_array().into()
}

pub const SUPERBLOCK_HASH_MESSAGE_LENGTH: usize = size_of::<BlockHash>();

pub fn extract_superblock_ts_from_header() -> Script {
    script! {
        for i in 0..4 { { 80 - 12 + 2 * i } OP_PICK }
        for _ in 1..4 {  { NMUL(1 << 8) } OP_ADD }
    }
}
