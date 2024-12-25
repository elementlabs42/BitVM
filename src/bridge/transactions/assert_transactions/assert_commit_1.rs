use std::collections::BTreeMap;

use bitcoin::{absolute, consensus, Amount, EcdsaSighashType, ScriptBuf, Transaction, TxOut};
use rand::seq::index;
use serde::{Deserialize, Serialize};

use crate::{
    bridge::{
        graphs::peg_out::CommitmentMessageId,
        transactions::signing::{
            populate_p2wsh_witness_with_signatures, push_p2wsh_script_to_witness,
            push_taproot_leaf_unlock_data_to_witness,
        },
    },
    chunker::common::RawWitness,
};

use super::{
    super::{
        super::{
            connectors::{base::*, connector_f_1::ConnectorF1},
            contexts::operator::OperatorContext,
            graphs::base::FEE_AMOUNT,
        },
        base::*,
        pre_signed::*,
    },
    utils::AssertCommit1ConnectorsE,
};

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct AssertCommit1Transaction {
    #[serde(with = "consensus::serde::With::<consensus::serde::Hex>")]
    tx: Transaction,
    #[serde(with = "consensus::serde::With::<consensus::serde::Hex>")]
    prev_outs: Vec<TxOut>,
    prev_scripts: Vec<ScriptBuf>,
}

impl PreSignedTransaction for AssertCommit1Transaction {
    fn tx(&self) -> &Transaction {
        &self.tx
    }

    fn tx_mut(&mut self) -> &mut Transaction {
        &mut self.tx
    }

    fn prev_outs(&self) -> &Vec<TxOut> {
        &self.prev_outs
    }

    fn prev_scripts(&self) -> &Vec<ScriptBuf> {
        &self.prev_scripts
    }
}

impl AssertCommit1Transaction {
    pub fn new(
        context: &OperatorContext,
        connectors_e: &AssertCommit1ConnectorsE,
        connector_f_1: &ConnectorF1,
        tx_inputs: Vec<Input>,
    ) -> Self {
        assert_eq!(
            tx_inputs.len(),
            connectors_e.connectors_num(),
            "inputs and connectors e don't match"
        );
        let mut this = Self::new_for_validation(connectors_e, connector_f_1, tx_inputs);

        this
    }

    pub fn new_for_validation(
        connectors_e: &AssertCommit1ConnectorsE,
        connector_f_1: &ConnectorF1,
        tx_inputs: Vec<Input>,
    ) -> Self {
        let mut inputs = vec![];
        let mut prev_outs = vec![];
        let mut prev_scripts = vec![];
        let mut total_output_amount = Amount::from_sat(0);

        for (connector_e, input) in (0..connectors_e.connectors_num())
            .map(|idx| connectors_e.get_connector_e(idx))
            .zip(tx_inputs)
        {
            inputs.push(connector_e.generate_tx_in(&input));
            prev_outs.push(TxOut {
                value: input.amount,
                script_pubkey: connector_e.generate_address().script_pubkey(),
            });
            prev_scripts.push(connector_e.generate_script());
            total_output_amount += input.amount;
        }
        total_output_amount -= Amount::from_sat(FEE_AMOUNT);

        let _output_0 = TxOut {
            value: total_output_amount,
            script_pubkey: connector_f_1.generate_address().script_pubkey(),
        };

        AssertCommit1Transaction {
            tx: Transaction {
                version: bitcoin::transaction::Version(2),
                lock_time: absolute::LockTime::ZERO,
                input: inputs,
                output: vec![_output_0],
            },
            prev_outs,
            prev_scripts,
        }
    }

    pub fn sign(
        &mut self,
        context: &OperatorContext,
        connectors_e: &AssertCommit1ConnectorsE,
        witnesses: Vec<RawWitness>,
    ) {
        assert_eq!(witnesses.len(), connectors_e.connectors_num());
        for (input_index, witness) in (0..connectors_e.connectors_num()).zip(witnesses) {
            let script = &self.prev_scripts()[input_index].clone();
            push_taproot_leaf_unlock_data_to_witness(self.tx_mut(), input_index, witness);
            push_p2wsh_script_to_witness(self.tx_mut(), input_index, script);
        }
    }
}

impl BaseTransaction for AssertCommit1Transaction {
    fn finalize(&self) -> Transaction {
        self.tx.clone()
    }
}
