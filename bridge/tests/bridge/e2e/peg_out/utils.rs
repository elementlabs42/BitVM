use bitcoin::{Address, Amount, Network, ScriptBuf};
use bitvm::chunker::disprove_execution::RawProof;
use bridge::{
    client::client::BitVMClient,
    graphs::base::{BaseGraph, DUST_AMOUNT, FEE_AMOUNT},
    scripts::generate_pay_to_pubkey_script_address,
    transactions::base::{
        Input, MIN_RELAY_FEE_ASSERT_COMMIT1, MIN_RELAY_FEE_ASSERT_COMMIT2,
        MIN_RELAY_FEE_ASSERT_FINAL, MIN_RELAY_FEE_ASSERT_INITIAL, MIN_RELAY_FEE_DISPROVE,
        MIN_RELAY_FEE_KICK_OFF_2, MIN_RELAY_FEE_PEG_OUT,
    },
};
use colored::Colorize;
use core::panic;

use crate::bridge::{
    faucet::{Faucet, FaucetType},
    helper::{
        generate_stub_outpoint, get_default_peg_out_event, wait_for_confirmation_with_message,
        wait_for_timelock_expiry,
    },
    setup::{setup_test, INITIAL_AMOUNT},
};

pub async fn create_peg_in_graph(
    network: Network,
    verifier_0: &mut BitVMClient,
    verifier_1: &mut BitVMClient,
    deposit_input: Input,
    depositor_evm_address: &String,
) -> String {
    println!("{}", "Creating PEG-IN graph...".bold().yellow());
    let graph_id = verifier_0
        .create_peg_in_graph(deposit_input, depositor_evm_address)
        .await;

    match verifier_0.broadcast_peg_in_deposit(&graph_id).await {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "peg-in deposit".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast peg-in deposit: {e}"),
    }
    wait_for_confirmation_with_message(network, Some("peg-in deposit tx")).await;

    println!("{}", "PEG-IN ceremony start".bold().yellow());
    println!("{}", "Generate verifier 0 nonces".bold().magenta());
    verifier_0.push_verifier_nonces(&graph_id);
    println!("{}", "Flush verifier 0 nonces".bold().magenta());
    verifier_0.flush().await;

    println!("{}", "Sync verifier 1".bold().blue());
    verifier_1.sync().await;
    println!("{}", "Generate verifier 1 nonces".bold().blue());
    verifier_1.push_verifier_nonces(&graph_id);
    println!("{}", "Flush verifier 1 nonces".bold().blue());
    verifier_1.flush().await;

    println!("{}", "Sync verifier 0".bold().magenta());
    verifier_0.sync().await;
    println!("{}", "Generate verifier 0 signatures".bold().magenta());
    verifier_0.push_verifier_signature(&graph_id);
    println!("{}", "Flush verifier 0 signatures".bold().magenta());
    verifier_0.flush().await;

    println!("{}", "Sync verifier 1".bold().blue());
    verifier_1.sync().await;
    println!("{}", "Generate verifier 1 signatures".bold().blue());
    verifier_1.push_verifier_signature(&graph_id);
    println!("{}", "Flush verifier 1 signatures".bold().blue());
    verifier_1.flush().await;
    println!("{}", "PEG-IN ceremony finish".bold().yellow());

    println!("{}", "Sync verifier 0".bold().magenta());
    verifier_0.sync().await;

    match verifier_0.broadcast_peg_in_confirm(&graph_id).await {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "peg-in confirm".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast peg-in confirm: {e}"),
    }
    wait_for_confirmation_with_message(network, Some("peg-in confirm tx")).await;

    graph_id
}

pub async fn create_peg_out_graph() -> (BitVMClient, BitVMClient, String, ScriptBuf, Input, Network)
{
    let config = setup_test().await;
    let mut verifier_0_operator_depositor = config.client_0;
    let mut verifier_1 = config.client_1;

    // verify funding inputs
    let mut funding_inputs: Vec<(&Address, Amount)> = Vec::new();

    let deposit_input_amount = Amount::from_sat(INITIAL_AMOUNT + FEE_AMOUNT);
    let deposit_funding_address = generate_pay_to_pubkey_script_address(
        config.depositor_context.network,
        &config.depositor_context.depositor_public_key,
    );
    funding_inputs.push((&deposit_funding_address, deposit_input_amount));

    let peg_out_input_amount = Amount::from_sat(INITIAL_AMOUNT + MIN_RELAY_FEE_PEG_OUT);
    let peg_out_funding_address = generate_pay_to_pubkey_script_address(
        config.operator_context.network,
        &config.operator_context.operator_public_key,
    );
    funding_inputs.push((&peg_out_funding_address, peg_out_input_amount));

    let peg_out_confirm_input_amount = Amount::from_sat(
        INITIAL_AMOUNT
          + MIN_RELAY_FEE_KICK_OFF_2
          + DUST_AMOUNT // connector 3 to take 1
          + MIN_RELAY_FEE_ASSERT_INITIAL
          + MIN_RELAY_FEE_ASSERT_COMMIT1
          + MIN_RELAY_FEE_ASSERT_COMMIT2
          + MIN_RELAY_FEE_ASSERT_FINAL
          + DUST_AMOUNT // connector 4 to take 2
          + MIN_RELAY_FEE_DISPROVE,
    );
    let peg_out_confirm_funding_address = generate_pay_to_pubkey_script_address(
        config.operator_context.network,
        &config.operator_context.operator_public_key,
    );
    funding_inputs.push((
        &peg_out_confirm_funding_address,
        peg_out_confirm_input_amount,
    ));

    let faucet = Faucet::new(FaucetType::EsploraRegtest);
    faucet
        .fund_inputs(&verifier_0_operator_depositor, &funding_inputs)
        .await;

    wait_for_confirmation_with_message(config.network, Some("funding inputs")).await;

    // create peg-in graph
    let peg_in_deposit_outpoint = generate_stub_outpoint(
        &verifier_0_operator_depositor,
        &deposit_funding_address,
        deposit_input_amount,
    )
    .await;

    let peg_in_graph_id = create_peg_in_graph(
        config.network,
        &mut verifier_0_operator_depositor,
        &mut verifier_1,
        Input {
            outpoint: peg_in_deposit_outpoint,
            amount: deposit_input_amount,
        },
        &config.depositor_evm_address,
    )
    .await;

    // create peg-out graph
    let peg_out_confirm_outpoint = generate_stub_outpoint(
        &verifier_0_operator_depositor,
        &peg_out_confirm_funding_address,
        peg_out_confirm_input_amount,
    )
    .await;
    verifier_0_operator_depositor.sync().await;

    println!("{}", "Creating PEG-OUT graph...".bold().yellow());
    let peg_out_graph_id = verifier_0_operator_depositor.create_peg_out_graph(
        &peg_in_graph_id,
        Input {
            outpoint: peg_out_confirm_outpoint,
            amount: peg_out_confirm_input_amount,
        },
        config.commitment_secrets,
    );

    println!("{}", "PEG-OUT ceremony start".bold().yellow());
    println!("{}", "Generate verifier 0 nonces".bold().magenta());
    verifier_0_operator_depositor.push_verifier_nonces(&peg_out_graph_id);
    println!("{}", "Flush verifier 0 nonces".bold().magenta());
    verifier_0_operator_depositor.flush().await;

    println!("{}", "Sync verifier 1".bold().blue());
    verifier_1.sync().await;
    println!("{}", "Generate verifier 1 nonces".bold().blue());
    verifier_1.push_verifier_nonces(&peg_out_graph_id);
    println!("{}", "Flush verifier 1 nonces".bold().blue());
    verifier_1.flush().await;

    println!("{}", "Sync verifier 0".bold().magenta());
    verifier_0_operator_depositor.sync().await;
    println!("{}", "Generate verifier 0 signatures".bold().magenta());
    verifier_0_operator_depositor.push_verifier_signature(&peg_out_graph_id);
    println!("{}", "Flush verifier 0 signatures".bold().magenta());
    verifier_0_operator_depositor.flush().await;

    println!("{}", "Sync verifier 1".bold().blue());
    verifier_1.sync().await;
    println!("{}", "Generate verifier 1 signatures".bold().blue());
    verifier_1.push_verifier_signature(&peg_out_graph_id);
    println!("{}", "Flush verifier 1 signatures".bold().blue());
    verifier_1.flush().await;
    println!("{}", "PEG-OUT ceremony finish".bold().yellow());

    let reward_address = generate_pay_to_pubkey_script_address(
        config.verifier_1_context.network,
        &config.verifier_1_context.verifier_public_key,
    );
    let reward_script = reward_address.script_pubkey();

    let peg_out_outpoint = generate_stub_outpoint(
        &verifier_0_operator_depositor,
        &peg_out_funding_address,
        peg_out_input_amount,
    )
    .await;

    (
        verifier_0_operator_depositor,
        verifier_1,
        peg_out_graph_id,
        reward_script,
        Input {
            outpoint: peg_out_outpoint,
            amount: peg_out_input_amount,
        },
        config.network,
    )
}

pub async fn broadcast_txs_for_disprove_scenario(
    network: Network,
    operator: &mut BitVMClient,
    verifier_1: &mut BitVMClient,
    peg_out_graph_id: &String,
    peg_out_input: Input,
    proof: &RawProof,
) {
    println!("{}", "Sync operator".bold().cyan());
    operator.sync().await;

    // peg_out_chain_event
    {
        let peg_out_graph;
        match operator
            .data_mut()
            .peg_out_graphs
            .iter_mut()
            .find(|x| x.id() == peg_out_graph_id)
        {
            Some(graph) => {
                peg_out_graph = graph;
            }
            None => panic!("Peg-out graph {peg_out_graph_id} not found"),
        };

        // set arbitrary peg_out_chain_event
        peg_out_graph.peg_out_chain_event = Some(get_default_peg_out_event());
    }

    match operator
        .broadcast_peg_out(peg_out_graph_id, peg_out_input)
        .await
    {
        Ok(txid) => println!("Broadcasted {} with txid {txid}", "peg-out".bold().green()),
        Err(e) => panic!("Failed to broadcast peg-out: {e}"),
    };
    wait_for_confirmation_with_message(network, Some("peg-out tx")).await;

    match operator.broadcast_peg_out_confirm(peg_out_graph_id).await {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "peg-out confirm".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast peg-out confirm: {e}"),
    }
    wait_for_confirmation_with_message(network, Some("peg-out confirm tx")).await;

    match operator.broadcast_kick_off_1(peg_out_graph_id).await {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "kick-off 1".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast kick-off 1: {e}"),
    }
    wait_for_timelock_expiry(network, Some("kick-off 1 connector 1")).await;

    match operator.broadcast_kick_off_2(peg_out_graph_id).await {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "kick-off 2".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast kick-off 2: {e}"),
    }
    wait_for_timelock_expiry(network, Some("kick-off 2 connector B")).await;

    match operator.broadcast_assert_initial(peg_out_graph_id).await {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "assert-initial".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast assert-initial: {e}"),
    }
    wait_for_confirmation_with_message(network, Some("assert-initial tx")).await;

    match operator
        .broadcast_assert_commit_1(peg_out_graph_id, proof)
        .await
    {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "assert-commit 1".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast assert-commit 1: {e}"),
    }
    match operator
        .broadcast_assert_commit_2(peg_out_graph_id, proof)
        .await
    {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "assert-commit 2".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast assert-commit 2: {e}"),
    }
    wait_for_confirmation_with_message(network, Some("assert-commit 1 and assert-commit 2 txs"))
        .await;

    match operator.broadcast_assert_final(peg_out_graph_id).await {
        Ok(txid) => println!(
            "Broadcasted {} with txid {txid}",
            "assert-final".bold().green()
        ),
        Err(e) => panic!("Failed to broadcast assert-final: {e}"),
    }
    wait_for_confirmation_with_message(network, Some("assert-final tx")).await;

    println!("{}", "Flush operator txs".bold().cyan());
    operator.flush().await;

    println!("{}", "Sync verifier 1".bold().blue());
    verifier_1.sync().await;
}
