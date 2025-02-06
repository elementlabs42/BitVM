use colored::Colorize;

use crate::bridge::helper::get_incorrect_proof;

use super::utils::{broadcast_txs_for_disprove_scenario, create_peg_out_graph};

#[tokio::test]
async fn test_disprove_success_1() {
    let (
        mut verifier_0_operator_depositor,
        mut verifier_1,
        peg_out_graph_id,
        reward_script,
        peg_out_input,
        network,
    ) = create_peg_out_graph().await;

    broadcast_txs_for_disprove_scenario(
        network,
        &mut verifier_0_operator_depositor,
        &mut verifier_1,
        &peg_out_graph_id,
        peg_out_input,
        &get_incorrect_proof(),
    )
    .await;

    match verifier_1
        .broadcast_disprove(&peg_out_graph_id, reward_script)
        .await
    {
        Ok(txid) => {
            println!("Broadcasted {} with txid {txid}", "disprove".bold().green());
            println!(
                "{}",
                "Succesfully disproved incorrect ZK proof".bold().green()
            );
        }
        Err(e) => panic!(
            "{}: {e}",
            "Failed to disprove incorrect ZK proof".bold().red()
        ),
    }
}
