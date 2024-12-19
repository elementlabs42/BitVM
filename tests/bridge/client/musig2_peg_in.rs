use std::time::Duration;

use bitcoin::Amount;

use bitvm::bridge::{
    graphs::base::FEE_AMOUNT, scripts::generate_pay_to_pubkey_script_address,
    transactions::base::Input,
};

use serial_test::serial;
use tokio::time::sleep;

use crate::bridge::{
    faucet::{Faucet, FaucetType},
    helper::{generate_stub_outpoint, TX_WAIT_TIME},
    setup::{setup_test, INITIAL_AMOUNT},
};

#[tokio::test]
#[serial]
async fn test_musig2_peg_in() {
    let config = setup_test().await;
    let mut depositor_operator_verifier_0_client = config.client_0;
    let mut verifier_1_client = config.client_1;

    // Depositor: generate graph
    let amount = Amount::from_sat(INITIAL_AMOUNT + FEE_AMOUNT);
    let depositor_funding_utxo_address = generate_pay_to_pubkey_script_address(
        config.depositor_context.network,
        &config.depositor_context.depositor_public_key,
    );
    let faucet = Faucet::new(FaucetType::EsploraRegtest);
    faucet
        .fund_input(&depositor_funding_utxo_address, amount)
        .await
        .wait()
        .await;
    let outpoint = generate_stub_outpoint(
        &depositor_operator_verifier_0_client,
        &depositor_funding_utxo_address,
        amount,
    )
    .await;

    let graph_id = depositor_operator_verifier_0_client
        .create_peg_in_graph(Input { outpoint, amount }, &config.depositor_evm_address)
        .await;
    println!("Depositor: Created new graph {graph_id}");

    println!("Depositor: Mining peg in deposit...");
    depositor_operator_verifier_0_client
        .broadcast_peg_in_deposit(&graph_id)
        .await;

    println!("Depositor: Saving state changes to remote...");
    depositor_operator_verifier_0_client.flush().await;

    // Verifier 0: push nonces
    println!("Verifier 0: Reading state from remote...");
    depositor_operator_verifier_0_client.sync().await;

    println!("Verifier 0: Generating nonces...");
    depositor_operator_verifier_0_client.push_verifier_nonces(&graph_id);

    println!("Verifier 0: Saving state changes to remote...");
    depositor_operator_verifier_0_client.flush().await;

    // Verifier 1: push nonces
    println!("Verifier 1: Reading state from remote...");
    verifier_1_client.sync().await;

    println!("Verifier 1: Generating nonces...");
    verifier_1_client.push_verifier_nonces(&graph_id);

    println!("Verifier 1: Saving state changes to remote...");
    verifier_1_client.flush().await;

    // Verifier 0: presign
    println!("Verifier 0: Reading state from remote...");
    depositor_operator_verifier_0_client.sync().await;

    println!("Verifier 0: Pre-signing...");
    depositor_operator_verifier_0_client.push_verifier_signature(&graph_id);

    println!("Verifier 0: Saving state changes to remote...");
    depositor_operator_verifier_0_client.flush().await;

    // Verifier 1: presign
    println!("Verifier 1: Reading state from remote...");
    verifier_1_client.sync().await;

    println!("Verifier 1: Pre-signing...");
    verifier_1_client.push_verifier_signature(&graph_id);

    println!("Verifier 1: Saving state changes to remote...");
    verifier_1_client.flush().await;

    // Operator: finalize & verify
    println!("Operator: Reading state from remote...");
    depositor_operator_verifier_0_client.sync().await;

    let timeout = Duration::from_secs(TX_WAIT_TIME);
    println!(
        "Waiting {:?} for peg-in deposit transaction to be mined...",
        timeout
    );
    sleep(timeout).await; // TODO: Replace this with a 'wait x amount of time till tx is mined' routine.
                          // See the relevant TODO in PegInGraph::confirm().

    println!("Depositor: Mining peg in confirm...");
    depositor_operator_verifier_0_client
        .broadcast_peg_in_confirm(&graph_id)
        .await;

    println!("Operator: Saving state changes to remote...");
    depositor_operator_verifier_0_client.flush().await;
}
