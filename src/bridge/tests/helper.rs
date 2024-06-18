use bitcoin::{Address, Amount, OutPoint};

use crate::bridge::client::BitVMClient;

pub async fn generate_stub_outpoint(
    client: &BitVMClient,
    funding_utxo_address: &Address,
    input_value: Amount,
) -> OutPoint {
    let funding_utxo = client
        .get_initial_utxo(funding_utxo_address.clone(), input_value)
        .await
        .unwrap_or_else(|| {
            panic!(
                "Fund {:?} with {} sats at https://faucet.mutinynet.com/",
                funding_utxo_address,
                input_value.to_sat()
            );
        });
    println!("txid: {}, vout: {}", funding_utxo.txid, funding_utxo.vout);
    OutPoint {
        txid: funding_utxo.txid,
        vout: funding_utxo.vout,
    }
}
