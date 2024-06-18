use crate::treepp::*;
use bitcoin::{absolute, Amount, Sequence, Transaction, TxIn, TxOut, Witness};

use super::super::context::BridgeContext;
use super::super::graph::FEE_AMOUNT;

use super::bridge::*;
use super::connector_z::*;
use super::helper::*;

pub struct PegInDepositTransaction {
    tx: Transaction,
    prev_outs: Vec<TxOut>,
    prev_scripts: Vec<Script>,
    evm_address: String,
}

impl PegInDepositTransaction {
    pub fn new(context: &BridgeContext, input0: Input, evm_address: String) -> Self {
        let n_of_n_pubkey = context
            .n_of_n_pubkey
            .expect("n_of_n_pubkey is required in context");
        let depositor_pubkey = context
            .depositor_pubkey
            .expect("depositor_pubkey is required in context");

        let _input0 = TxIn {
            previous_output: input0.outpoint,
            script_sig: Script::new(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        };

        let total_input_amount = input0.amount - Amount::from_sat(FEE_AMOUNT);

        let _output0 = TxOut {
            value: total_input_amount,
            script_pubkey: generate_address(&evm_address, &n_of_n_pubkey, &depositor_pubkey)
                .script_pubkey(),
        };

        PegInDepositTransaction {
            tx: Transaction {
                version: bitcoin::transaction::Version(2),
                lock_time: absolute::LockTime::ZERO,
                input: vec![_input0],
                output: vec![_output0],
            },
            prev_outs: vec![],    // TODO
            prev_scripts: vec![], // TODO
            evm_address,
        }
    }
}

impl BridgeTransaction for PegInDepositTransaction {
    fn pre_sign(&mut self, context: &BridgeContext) {
        todo!()
        // TODO presign leaf0
        // TODO depositor presign leaf1
    }

    fn finalize(&self, context: &BridgeContext) -> Transaction {
        // TODO n-of-n finish presign leaf1
        self.tx.clone()
    }
}
