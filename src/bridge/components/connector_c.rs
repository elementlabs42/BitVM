use crate::treepp::*;
use bitcoin::{
    hashes::{ripemd160, Hash},
    key::Secp256k1,
    taproot::{TaprootBuilder, TaprootSpendInfo},
    Address, Network, TxIn, XOnlyPublicKey,
};

use super::connector::*;
use super::helper::*;

// Specialized for assert leaves currently.
pub type LockScript = fn(index: u32) -> Script;

pub type UnlockWitness = fn(index: u32) -> Vec<Vec<u8>>;

pub struct AssertLeaf {
    pub lock: LockScript,
    pub unlock: UnlockWitness,
}

pub struct ConnectorC {
    pub network: Network,
    pub n_of_n_taproot_public_key: XOnlyPublicKey,
}

impl ConnectorC {
    pub fn new(network: Network, n_of_n_taproot_public_key: &XOnlyPublicKey) -> Self {
        ConnectorC {
            network,
            n_of_n_taproot_public_key: n_of_n_taproot_public_key.clone(),
        }
    }

    // Leaf[i] for some i in 1,2,…1000: spendable by multisig of OPK and VPK[1…N] plus the condition that f_{i}(z_{i-1})!=z_i
    pub fn assert_leaf(&self) -> AssertLeaf {
        AssertLeaf {
            lock: |index| {
                script! {
                    OP_RIPEMD160
                    { ripemd160::Hash::hash(format!("SECRET_{}", index).as_bytes()).as_byte_array().to_vec() }
                    OP_EQUALVERIFY
                    { index }
                    OP_DROP
                    OP_TRUE
                }
            },
            unlock: |index| vec![format!("SECRET_{}", index).as_bytes().to_vec()],
        }
    }

    pub fn generate_assert_leaves(&self) -> Vec<Script> {
        // TODO: Scripts with n_of_n_public_key and one of the commitments disprove leaves in each leaf (Winternitz signatures)
        let mut leaves = Vec::with_capacity(1000);
        let locking_template = self.assert_leaf().lock;
        for i in 0..1000 {
            leaves.push(locking_template(i));
        }
        leaves
    }

    pub fn generate_taproot_leaf_tx_in(&self, input: &Input) -> TxIn {
        generate_default_tx_in(input)
    }

    // Returns the TaprootSpendInfo for the Commitment Taptree and the corresponding pre_sign_output
    pub fn generate_taproot_spend_info(&self) -> TaprootSpendInfo {
        let disprove_scripts = self.generate_assert_leaves();
        let script_weights = disprove_scripts.iter().map(|script| (1, script.clone()));

        TaprootBuilder::with_huffman_tree(script_weights)
            .expect("Unable to add assert leaves")
            // Finalizing with n_of_n_public_key allows the key-path spend with the
            // n_of_n
            .finalize(&Secp256k1::new(), self.n_of_n_taproot_public_key)
            .expect("Unable to finalize assert transaction connector c taproot")
    }

    pub fn generate_taproot_address(&self) -> Address {
        Address::p2tr_tweaked(
            self.generate_taproot_spend_info().output_key(),
            self.network,
        )
    }
}