use bitcoin::{Network, Transaction};

use crate::chunker::common::RawWitness;

const NUM_BLOCKS_REGTEST: u32 = 3;
const NUM_BLOCKS_TESTNET: u32 = 3;

pub fn num_blocks_per_network(network: Network, mainnet_num_blocks: u32) -> u32 {
    match network {
        Network::Bitcoin => mainnet_num_blocks,
        Network::Regtest => NUM_BLOCKS_REGTEST,
        _ => NUM_BLOCKS_TESTNET, // Testnet, Signet
    }
}

pub fn get_commit_from_assert_commit_tx(assert_commit_tx: &Transaction) -> Vec<RawWitness> {
  let mut assert_commit_witness = Vec::new();
  for input in assert_commit_tx.input.iter() {
    // remove script and control block from witness
    let witness = remove_script_and_control_block_from_witness(input.witness.to_vec());
    assert_commit_witness.push(witness);
  }

  assert_commit_witness
}

fn remove_script_and_control_block_from_witness(mut witness: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
  witness.truncate(witness.len() - 2);

  witness
}
