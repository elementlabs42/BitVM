use bitcoin::Network;

pub const REGTEST_ESPLORA_URL: &str = "http://localhost:8094/regtest/api/";
pub const ALPEN_SIGNET_ESPLORA_URL: &str =
    "https://esploraapi53d3659b.devnet-annapurna.stratabtc.org/";

pub fn get_esplora_url(network: Network) -> &'static str {
    match network {
        Network::Regtest => REGTEST_ESPLORA_URL,
        _ => ALPEN_SIGNET_ESPLORA_URL,
    }
}
