use crate::treepp::*;
use bitcoin::{Address, Amount, Network, OutPoint, XOnlyPublicKey};

pub fn generate_burn_script() -> Script {
    script! {
        OP_RETURN // TODO replace with Satoshi's address (Unspendable pubkey)
    }
}

pub fn generate_burn_script_address() -> Address {
    Address::p2wsh(&generate_burn_script(), Network::Testnet)
}

pub fn generate_timelock_script(pubkey: &XOnlyPublicKey, weeks: u32) -> Script {
    script! {
      { NUM_BLOCKS_PER_WEEK * weeks }
      OP_CSV
      OP_DROP
      { *pubkey }
      OP_CHECKSIG
    }
}

pub fn generate_timelock_script_address(pubkey: &XOnlyPublicKey, weeks: u32) -> Address {
    Address::p2wsh(&generate_timelock_script(pubkey, weeks), Network::Testnet)
}

pub fn generate_pay_to_pubkey_script(pubkey: &XOnlyPublicKey) -> Script {
    script! {
        { *pubkey }
        OP_CHECKSIG
    }
}

pub fn generate_pay_to_pubkey_script_address(pubkey: &XOnlyPublicKey) -> Address {
    Address::p2wsh(&generate_pay_to_pubkey_script(pubkey), Network::Testnet)
}

pub struct Input {
    pub outpoint: OutPoint,
    pub amount: Amount,
}

pub const NUM_BLOCKS_PER_WEEK: u32 = 1008;
