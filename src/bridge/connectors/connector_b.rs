use std::collections::HashMap;

use bitcoin::{
    block::Header,
    key::Secp256k1,
    opcodes::all::{OP_ADD, OP_FROMALTSTACK, OP_LESSTHAN, OP_TOALTSTACK},
    taproot::{TaprootBuilder, TaprootSpendInfo},
    Address, Network, ScriptBuf, TxIn, XOnlyPublicKey,
};
use bitcoin_script::script;
use serde::{Deserialize, Serialize};

use crate::{
    bridge::{
        constants::START_TIME_MESSAGE_LENGTH,
        graphs::peg_out::CommitmentMessageId,
        superblock::{
            extract_superblock_ts_from_header, SUPERBLOCK_HASH_MESSAGE_LENGTH,
            SUPERBLOCK_MESSAGE_LENGTH,
        },
        transactions::signing_winternitz::{
            winternitz_message_checksig, winternitz_message_checksig_verify, WinternitzPublicKey,
            LOG_D,
        },
    },
    hash::sha256::{sha256, sha256_32bytes},
    signatures::utils::digits_to_number,
};

use super::{
    super::{
        constants::NUM_BLOCKS_PER_3_DAYS, scripts::*, transactions::base::Input,
        utils::num_blocks_per_network,
    },
    base::*,
};

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct ConnectorB {
    pub network: Network,
    pub n_of_n_taproot_public_key: XOnlyPublicKey,
    pub commitment_public_keys: HashMap<CommitmentMessageId, WinternitzPublicKey>,
    pub num_blocks_timelock_1: u32,
}

impl ConnectorB {
    pub fn new(
        network: Network,
        n_of_n_taproot_public_key: &XOnlyPublicKey,
        commitment_public_keys: &HashMap<CommitmentMessageId, WinternitzPublicKey>,
    ) -> Self {
        ConnectorB {
            network,
            n_of_n_taproot_public_key: n_of_n_taproot_public_key.clone(),
            commitment_public_keys: commitment_public_keys.clone(),
            num_blocks_timelock_1: num_blocks_per_network(network, NUM_BLOCKS_PER_3_DAYS),
        }
    }

    fn generate_taproot_leaf_0_script(&self) -> ScriptBuf {
        const TWO_WEEKS_IN_SECONDS: u32 = 60 * 60 * 24 * 14;
        let superblock_hash_public_key =
            &self.commitment_public_keys[&CommitmentMessageId::SuperblockHash];
        let start_time_public_key = &self.commitment_public_keys[&CommitmentMessageId::StartTime];

        // Expected witness:
        // n-of-n Schnorr siganture
        // SB' (byte stream)
        // Committed start time (Winternitz sig)
        // Committed SB hash (Winternitz sig)

        script! {
            // Verify superblock hash comitment sig
            { winternitz_message_checksig(&superblock_hash_public_key) }
            // Convert committed SB hash to number and push it to altstack
            { digits_to_number::<{ SUPERBLOCK_HASH_MESSAGE_LENGTH * 2 }, { LOG_D as usize }>() }
            OP_TOALTSTACK

            // Verify start time comitment sig
            { winternitz_message_checksig(&start_time_public_key) }
            // Convert committed start time to number and push it to altstack
            { digits_to_number::<{ START_TIME_MESSAGE_LENGTH * 2 }, { LOG_D as usize }>() }
            OP_TOALTSTACK

            // Calculate SB' hash and push it to altstack
            { sha256(SUPERBLOCK_MESSAGE_LENGTH) }
            { sha256_32bytes() }
            OP_TOALTSTACK

            extract_superblock_ts_from_header

            // SB'.time > start_time
            OP_DUP
            OP_FROMALTSTACK // Stack: SB'.time | SB.time
            OP_GREATERTHAN OP_VERIFY

            // SB'.time < start_time + 2 weeks
            // get start_time here again
            { TWO_WEEKS_IN_SECONDS} OP_ADD // Stack: SB'.time | start_time + 2 weeks
            OP_LESSTHAN OP_VERIFY

            // SB'.weight > SB.weight
            OP_FROMALTSTACK
            OP_FROMALTSTACK // Stack: SB'.weight | SB.weight
            OP_LESSTHAN OP_VERIFY // We're comparing hashes as numbers; smaller number = bigger weight

            { self.n_of_n_taproot_public_key }
            OP_CHECKSIG
        }
        .compile()
    }

    fn generate_taproot_leaf_0_tx_in(&self, input: &Input) -> TxIn { generate_default_tx_in(input) }

    fn generate_taproot_leaf_1_script(&self) -> ScriptBuf {
        generate_timelock_taproot_script(
            &self.n_of_n_taproot_public_key,
            self.num_blocks_timelock_1,
        )
    }

    fn generate_taproot_leaf_1_tx_in(&self, input: &Input) -> TxIn {
        generate_timelock_tx_in(input, self.num_blocks_timelock_1)
    }

    fn generate_taproot_leaf_2_script(&self) -> ScriptBuf {
        // TODO commit to super block
        generate_pay_to_pubkey_taproot_script(&self.n_of_n_taproot_public_key)
    }

    fn generate_taproot_leaf_2_tx_in(&self, input: &Input) -> TxIn { generate_default_tx_in(input) }
}

impl TaprootConnector for ConnectorB {
    fn generate_taproot_leaf_script(&self, leaf_index: u32) -> ScriptBuf {
        match leaf_index {
            0 => self.generate_taproot_leaf_0_script(),
            1 => self.generate_taproot_leaf_1_script(),
            2 => self.generate_taproot_leaf_2_script(),
            _ => panic!("Invalid leaf index."),
        }
    }

    fn generate_taproot_leaf_tx_in(&self, leaf_index: u32, input: &Input) -> TxIn {
        match leaf_index {
            0 => self.generate_taproot_leaf_0_tx_in(input),
            1 => self.generate_taproot_leaf_1_tx_in(input),
            2 => self.generate_taproot_leaf_2_tx_in(input),
            _ => panic!("Invalid leaf index."),
        }
    }

    fn generate_taproot_spend_info(&self) -> TaprootSpendInfo {
        TaprootBuilder::new()
            .add_leaf(2, self.generate_taproot_leaf_0_script())
            .expect("Unable to add leaf 0")
            .add_leaf(2, self.generate_taproot_leaf_1_script())
            .expect("Unable to add leaf 1")
            .add_leaf(1, self.generate_taproot_leaf_2_script())
            .expect("Unable to add leaf 2")
            .finalize(&Secp256k1::new(), self.n_of_n_taproot_public_key)
            .expect("Unable to finalize taproot")
    }

    fn generate_taproot_address(&self) -> Address {
        Address::p2tr_tweaked(
            self.generate_taproot_spend_info().output_key(),
            self.network,
        )
    }
}
