use crate::treepp::*;
use bitcoin::{
    absolute,
    key::Keypair,
    secp256k1::Message,
    sighash::{Prevouts, SighashCache},
    taproot::LeafVersion,
    Amount, OutPoint, Sequence, TapLeafHash, TapSighashType,
    Transaction, TxIn, TxOut, Witness,
};

use super::super::context::BridgeContext;
use super::super::graph::{FEE_AMOUNT, N_OF_N_SECRET};

use super::connector_b::*;
use super::bridge::*;
use super::helper::*;
pub struct BurnTransaction {
    tx: Transaction,
    prev_outs: Vec<TxOut>,
    script_index: u32,
}

impl BurnTransaction {
    pub fn new(
        context: &BridgeContext,
        connector_b: OutPoint,
        pre_sign: OutPoint,
        connector_b_value: Amount,
        pre_sign_value: Amount,
        script_index: u32,
    ) -> Self {
        let n_of_n_pubkey = context
            .n_of_n_pubkey
            .expect("n_of_n_pubkey required in context");
        let unspendable_pubkey = context
          .unspendable_pubkey
          .expect("unspendable_pubkey required in context");

        let burn_output = TxOut {
            value: (connector_b_value - Amount::from_sat(FEE_AMOUNT)) * 100 / 95,
            script_pubkey: connector_b_address(unspendable_pubkey).script_pubkey(),
        };

        let connector_b_input = TxIn {
            previous_output: connector_b,
            script_sig: Script::new(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        };

        let pre_sign_input = TxIn {
            previous_output: pre_sign,
            script_sig: Script::new(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        };

        BurnTransaction {
            tx: Transaction {
                version: bitcoin::transaction::Version(2),
                lock_time: absolute::LockTime::ZERO,
                input: vec![pre_sign_input, connector_b_input],
                output: vec![burn_output],
            },
            prev_outs: vec![
                TxOut {
                    value: pre_sign_value,
                    script_pubkey: connector_b_pre_sign_address(n_of_n_pubkey).script_pubkey(),
                },
                TxOut {
                    value: connector_b_value,
                    script_pubkey: connector_b_address(n_of_n_pubkey).script_pubkey(),
                },
            ],
            script_index,
        }
    }
}

impl BridgeTransaction for BurnTransaction {
    //TODO: Real presign
    fn pre_sign(&mut self, context: &BridgeContext) {
        let n_of_n_key = Keypair::from_seckey_str(&context.secp, N_OF_N_SECRET).unwrap();
        let n_of_n_pubkey = context
            .n_of_n_pubkey
            .expect("n_of_n_pubkey required in context");

        // Create the signature with n_of_n_key as part of the setup
        let mut sighash_cache = SighashCache::new(&self.tx);
        let prevouts = Prevouts::All(&self.prev_outs);
        let prevout_leaf = (
            generate_pre_sign_script(n_of_n_pubkey),
            LeafVersion::TapScript,
        );

        // Use Single to sign only the burn output with the n_of_n_key
        let sighash_type = TapSighashType::Single;
        let leaf_hash =
            TapLeafHash::from_script(prevout_leaf.0.clone().as_script(), LeafVersion::TapScript);
        let sighash = sighash_cache
            .taproot_script_spend_signature_hash(0, &prevouts, leaf_hash, sighash_type)
            .expect("Failed to construct sighash");

        let msg = Message::from(sighash);
        let signature = context.secp.sign_schnorr_no_aux_rand(&msg, &n_of_n_key);

        let signature_with_type = bitcoin::taproot::Signature {
            signature,
            sighash_type,
        };

        // Fill in the pre_sign/checksig input's witness
        let spend_info = connector_b_spend_info(n_of_n_pubkey).0;
        let control_block = spend_info
            .control_block(&prevout_leaf)
            .expect("Unable to create Control block");
        self.tx.input[0].witness.push(signature_with_type.to_vec());
        self.tx.input[0].witness.push(prevout_leaf.0.to_bytes());
        self.tx.input[0].witness.push(control_block.serialize());
    }

    fn finalize(&self, context: &BridgeContext) -> Transaction {
        let n_of_n_pubkey = context
            .n_of_n_pubkey
            .expect("n_of_n_pubkey required in context");

        // TODO fill in proper tx info

        // let prevout_leaf = (
        //     (assert_leaf().lock)(self.script_index),
        //     LeafVersion::TapScript,
        // );
        // let spend_info = connector_b_spend_info(n_of_n_pubkey).1;
        // let control_block = spend_info
        //     .control_block(&prevout_leaf)
        //     .expect("Unable to create Control block");

        // Push the unlocking values, script and control_block onto the witness.
        let mut tx = self.tx.clone();
        // // Unlocking script
        // let mut witness_vec = (assert_leaf().unlock)(self.script_index);
        // // Script and Control block
        // witness_vec.extend_from_slice(&[prevout_leaf.0.to_bytes(), control_block.serialize()]);

        // tx.input[1].witness = Witness::from(witness_vec);
        tx
    }
}