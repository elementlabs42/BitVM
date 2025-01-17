use std::str::FromStr;

use bitcoin::{Address, Amount, OutPoint, Txid};
use bitvm::bridge::{
    client::{
        chain::chain::{Chain, PegOutEvent},
        client::BitVMClient,
    },
    graphs::{
        base::{max, BaseGraph, DUST_AMOUNT, PEG_IN_FEE, PEG_OUT_FEE_FOR_TAKE_1},
        peg_in::PegInGraph,
        peg_out::{CommitmentMessageId, PegOutGraph},
    },
    scripts::{
        generate_p2pkh_address, generate_pay_to_pubkey_script,
        generate_pay_to_pubkey_script_address,
    },
    transactions::{
        base::{
            Input, InputWithScript, MIN_RELAY_FEE_CHALLENGE, MIN_RELAY_FEE_KICK_OFF_1,
            MIN_RELAY_FEE_KICK_OFF_TIMEOUT, MIN_RELAY_FEE_PEG_IN_CONFIRM,
            MIN_RELAY_FEE_PEG_IN_REFUND, MIN_RELAY_FEE_PEG_OUT, MIN_RELAY_FEE_PEG_OUT_CONFIRM,
            MIN_RELAY_FEE_START_TIME, MIN_RELAY_FEE_START_TIME_TIMEOUT,
        },
        pre_signed::PreSignedTransaction,
    },
};
use num_traits::ToPrimitive;

use crate::bridge::{
    faucet::{Faucet, FaucetType},
    helper::{
        check_tx_output_sum, find_peg_in_graph_by_peg_out, generate_stub_outpoint,
        get_lock_scripts_cached, get_reward_amount, random_hex, wait_for_confirmation,
        wait_timelock_expiry,
    },
    mock::chain::mock::MockAdaptor,
    setup::{setup_test, INITIAL_AMOUNT, ONE_HUNDRED},
};

#[tokio::test]
async fn test_peg_in_fees() {
    let mut config = setup_test().await;
    let faucet = Faucet::new(FaucetType::EsploraRegtest);

    let amount = Amount::from_sat(INITIAL_AMOUNT + PEG_IN_FEE);
    let address = generate_pay_to_pubkey_script_address(
        config.depositor_context.network,
        &config.depositor_context.depositor_public_key,
    );
    faucet.fund_input(&address, amount).await.wait().await;
    let peg_in_outpoint = generate_stub_outpoint(&config.client_0, &address, amount).await;

    let peg_in_input = Input {
        outpoint: peg_in_outpoint,
        amount,
    };
    let peg_in_graph_id = config
        .client_0
        .create_peg_in_graph(peg_in_input, &config.depositor_evm_address)
        .await;

    let esplora_client = config.client_0.esplora.clone();

    let peg_in_graph = get_peg_in_graph_mut(&mut config.client_0, peg_in_graph_id.clone());
    let peg_in_deposit_tx = peg_in_graph.deposit(&esplora_client).await.unwrap();
    check_tx_output_sum(
        INITIAL_AMOUNT + max(MIN_RELAY_FEE_PEG_IN_CONFIRM, MIN_RELAY_FEE_PEG_IN_REFUND),
        &peg_in_deposit_tx,
    );
    let deposit_result = esplora_client.broadcast(&peg_in_deposit_tx).await;
    println!("Deposit result: {deposit_result:?}");
    assert!(deposit_result.is_ok());
    config
        .client_0
        .process_peg_in_as_verifier(&peg_in_graph_id)
        .await;
    config.client_0.flush().await;

    config.client_1.sync().await;
    config
        .client_1
        .process_peg_in_as_verifier(&peg_in_graph_id)
        .await;

    let peg_in_graph = get_peg_in_graph_mut(&mut config.client_0, peg_in_graph_id.clone());
    wait_timelock_expiry(config.network, Some("peg-in deposit connector z")).await;
    let peg_in_confirm_tx = peg_in_graph.confirm(&esplora_client).await.unwrap();
    check_tx_output_sum(
        INITIAL_AMOUNT + max(MIN_RELAY_FEE_PEG_IN_CONFIRM, MIN_RELAY_FEE_PEG_IN_REFUND)
            - MIN_RELAY_FEE_PEG_IN_CONFIRM,
        &peg_in_confirm_tx,
    );

    let peg_in_refund_tx = peg_in_graph.refund(&esplora_client).await.unwrap();
    check_tx_output_sum(
        INITIAL_AMOUNT + max(MIN_RELAY_FEE_PEG_IN_CONFIRM, MIN_RELAY_FEE_PEG_IN_REFUND)
            - MIN_RELAY_FEE_PEG_IN_REFUND,
        &peg_in_refund_tx,
    );
}

#[tokio::test]
async fn test_peg_out_fees() {
    let mut config = setup_test().await;

    let peg_in_amount = Amount::from_sat(INITIAL_AMOUNT + PEG_OUT_FEE_FOR_TAKE_1);
    let peg_in_outpoint = OutPoint {
        txid: Txid::from_str(&random_hex(32)).unwrap(),
        vout: 0,
    };
    let input = Input {
        outpoint: peg_in_outpoint,
        amount: peg_in_amount,
    };
    let peg_in_graph_id = config
        .client_0
        .create_peg_in_graph(input, &config.depositor_evm_address)
        .await;

    let peg_out_amount = Amount::from_sat(ONE_HUNDRED + MIN_RELAY_FEE_PEG_OUT);
    let reward_amount = get_reward_amount(ONE_HUNDRED);
    let peg_out_confirm_input_amount = Amount::from_sat(reward_amount + PEG_OUT_FEE_FOR_TAKE_1);
    let faucet = Faucet::new(FaucetType::EsploraRegtest);
    let mut funding_inputs: Vec<(&Address, Amount)> = vec![];
    let address = generate_pay_to_pubkey_script_address(
        config.operator_context.network,
        &config.operator_context.operator_public_key,
    );
    funding_inputs.push((&address, peg_out_amount));
    funding_inputs.push((&address, peg_out_confirm_input_amount));
    faucet
        .fund_inputs(&config.client_0, &funding_inputs)
        .await
        .wait()
        .await;

    let peg_out_outpoint = generate_stub_outpoint(&config.client_0, &address, peg_out_amount).await;
    let peg_out_confirm_outpoint =
        generate_stub_outpoint(&config.client_0, &address, peg_out_confirm_input_amount).await;

    let peg_out_input = Input {
        outpoint: peg_out_outpoint,
        amount: peg_out_amount,
    };

    config.client_0.sync().await;
    let peg_out_graph_id = config
        .client_0
        .create_peg_out_graph(
            &peg_in_graph_id,
            Input {
                outpoint: peg_out_confirm_outpoint,
                amount: peg_out_confirm_input_amount,
            },
            config.commitment_secrets,
            get_lock_scripts_cached,
        )
        .await;

    config.client_0.push_verifier_nonces(&peg_out_graph_id);
    config.client_0.flush().await;

    config.client_1.sync().await;
    config.client_1.push_verifier_nonces(&peg_out_graph_id);
    config.client_1.flush().await;

    config.client_0.sync().await;
    config.client_0.push_verifier_signature(&peg_out_graph_id);
    config.client_0.flush().await;

    config.client_1.sync().await;
    config.client_1.push_verifier_signature(&peg_out_graph_id);
    config.client_1.flush().await;

    let esplora_client = config.client_0.esplora.clone();

    let peg_in_graph = find_peg_in_graph_by_peg_out(&config.client_0, &peg_out_graph_id).unwrap();
    let peg_in_confirm_tx = peg_in_graph.peg_in_confirm_transaction_ref().tx();
    let peg_in_confirm_vout: usize = 0;
    let peg_in_confirm_amount = peg_in_confirm_tx.output[peg_in_confirm_vout].value;
    let mut mock_adaptor = MockAdaptor::new();
    mock_adaptor.peg_out_init_events = vec![PegOutEvent {
        source_outpoint: OutPoint {
            txid: peg_in_graph.peg_in_confirm_transaction.tx().compute_txid(),
            vout: peg_in_confirm_vout.to_u32().unwrap(),
        },
        amount: peg_in_confirm_amount,
        timestamp: 1722328130u32,
        withdrawer_chain_address: config.withdrawer_evm_address,
        withdrawer_destination_address: generate_p2pkh_address(
            config.withdrawer_context.network,
            &config.withdrawer_context.withdrawer_public_key,
        )
        .to_string(),
        withdrawer_public_key_hash: config
            .withdrawer_context
            .withdrawer_public_key
            .pubkey_hash(),
        operator_public_key: config.operator_context.operator_public_key,
        tx_hash: [0u8; 32].into(), // 32 bytes 0
    }];
    let mut chain_adaptor = Chain::new();
    chain_adaptor.init_default(Box::new(mock_adaptor));
    config.client_0.set_chain_adaptor(chain_adaptor);
    config.client_0.sync_l2().await;

    let peg_out_graph = get_peg_out_graph_mut(&mut config.client_0, peg_out_graph_id.clone());
    let peg_out_tx = peg_out_graph
        .peg_out(&esplora_client, &config.operator_context, peg_out_input)
        .await
        .unwrap();
    check_tx_output_sum(ONE_HUNDRED, &peg_out_tx);
    let peg_out_result = esplora_client.broadcast(&peg_out_tx).await;
    wait_for_confirmation().await;
    println!("peg out tx result: {:?}\n", peg_out_result);
    assert!(peg_out_result.is_ok());

    let peg_out_confirm_tx = peg_out_graph
        .peg_out_confirm(&esplora_client)
        .await
        .unwrap();
    check_tx_output_sum(
        reward_amount + PEG_OUT_FEE_FOR_TAKE_1 - MIN_RELAY_FEE_PEG_OUT_CONFIRM,
        &peg_out_confirm_tx,
    );
    let peg_out_confirm_result = esplora_client.broadcast(&peg_out_confirm_tx).await;
    wait_for_confirmation().await;
    println!("peg out confirm tx result: {:?}\n", peg_out_confirm_result);
    assert!(peg_out_confirm_result.is_ok());

    let private_data = config.client_0.private_data_ref();
    let secrets_map = private_data.commitment_secrets[&config.operator_context.operator_public_key]
        [&peg_out_graph_id]
        .clone();
    let peg_out_graph = get_peg_out_graph_mut(&mut config.client_0, peg_out_graph_id.clone());
    let kick_off_1_tx = peg_out_graph
        .kick_off_1(
            &esplora_client,
            &config.operator_context,
            &secrets_map[&CommitmentMessageId::PegOutTxIdSourceNetwork],
            &secrets_map[&CommitmentMessageId::PegOutTxIdDestinationNetwork],
        )
        .await
        .unwrap();
    check_tx_output_sum(
        reward_amount + PEG_OUT_FEE_FOR_TAKE_1
            - MIN_RELAY_FEE_PEG_OUT_CONFIRM
            - MIN_RELAY_FEE_KICK_OFF_1,
        &kick_off_1_tx,
    );
    println!(
        "kick off 1 outputs: {:?}",
        kick_off_1_tx
            .output
            .iter()
            .map(|o| o.value.to_sat())
            .collect::<Vec<u64>>()
    );
    let kick_off_1_result = esplora_client.broadcast(&kick_off_1_tx).await;
    wait_for_confirmation().await;
    println!(
        "kick off 1 tx result: {:?}, {:?}\n",
        kick_off_1_result,
        kick_off_1_tx.compute_txid()
    );
    assert!(kick_off_1_result.is_ok());

    let start_time_tx = peg_out_graph
        .start_time(
            &esplora_client,
            &config.operator_context,
            &secrets_map[&CommitmentMessageId::StartTime],
        )
        .await
        .unwrap();
    check_tx_output_sum(DUST_AMOUNT, &start_time_tx);

    wait_timelock_expiry(config.network, Some("kick off 1 connector 1")).await;
    let start_time_timeout_tx = peg_out_graph
        .start_time_timeout(
            &esplora_client,
            generate_pay_to_pubkey_script(&config.depositor_context.depositor_public_key),
        )
        .await
        .unwrap();
    check_tx_output_sum(
        reward_amount + PEG_OUT_FEE_FOR_TAKE_1
            - MIN_RELAY_FEE_PEG_OUT_CONFIRM
            - MIN_RELAY_FEE_KICK_OFF_1
            - MIN_RELAY_FEE_START_TIME_TIMEOUT
            - DUST_AMOUNT,
        &start_time_timeout_tx,
    );

    let challenge_input_amount = Amount::from_sat(peg_out_graph.min_crowdfunding_amount() + 1);
    let challenge_funding_utxo_address = generate_pay_to_pubkey_script_address(
        config.network,
        &config.depositor_context.depositor_public_key,
    );
    faucet
        .fund_input(&challenge_funding_utxo_address, challenge_input_amount)
        .await
        .wait()
        .await;

    let challenge_funding_outpoint = generate_stub_outpoint(
        &config.client_0,
        &challenge_funding_utxo_address,
        challenge_input_amount,
    )
    .await;
    let depositor_pubkey_script =
        generate_pay_to_pubkey_script(&config.depositor_context.depositor_public_key);
    let challenge_crowdfunding_inputs = vec![InputWithScript {
        outpoint: challenge_funding_outpoint,
        amount: challenge_input_amount,
        script: &depositor_pubkey_script,
    }];

    let peg_out_graph = get_peg_out_graph_mut(&mut config.client_0, peg_out_graph_id.clone());
    let challenge_tx = peg_out_graph
        .challenge(
            &esplora_client,
            &config.depositor_context,
            &challenge_crowdfunding_inputs,
            &config.depositor_context.depositor_keypair,
            depositor_pubkey_script.clone(),
        )
        .await
        .unwrap();
    // crowdfunding discrepency less than dust will be lost as relay fee
    check_tx_output_sum(
        challenge_input_amount.to_sat() - 1 + DUST_AMOUNT - MIN_RELAY_FEE_CHALLENGE,
        &challenge_tx,
    );

    let reward_address = generate_pay_to_pubkey_script_address(
        config.withdrawer_context.network,
        &config.withdrawer_context.withdrawer_public_key,
    );
    let kick_off_timeout_tx = peg_out_graph
        .kick_off_timeout(&esplora_client, reward_address.script_pubkey())
        .await
        .unwrap();
    check_tx_output_sum(
        reward_amount + PEG_OUT_FEE_FOR_TAKE_1
            - MIN_RELAY_FEE_PEG_OUT_CONFIRM
            - MIN_RELAY_FEE_KICK_OFF_1
            - MIN_RELAY_FEE_KICK_OFF_TIMEOUT
            - DUST_AMOUNT * 2
            - MIN_RELAY_FEE_START_TIME,
        &kick_off_timeout_tx,
    );

    //TODO: kick off 2 and subsequent txns
}

// TODO: consider making the graph getter in client public after refactor
fn get_peg_in_graph_mut(client: &mut BitVMClient, id: String) -> &mut PegInGraph {
    client
        .data_mut_ref()
        .peg_in_graphs
        .iter_mut()
        .find(|graph| graph.id().eq(&id))
        .unwrap()
}

// TODO: consider making the graph getter in client public after refactor
fn get_peg_out_graph_mut(client: &mut BitVMClient, id: String) -> &mut PegOutGraph {
    client
        .data_mut_ref()
        .peg_out_graphs
        .iter_mut()
        .find(|graph| graph.id().eq(&id))
        .unwrap()
}
