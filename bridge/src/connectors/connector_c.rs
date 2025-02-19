use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    client::{
        files::BRIDGE_DATA_DIRECTORY_NAME,
        memory_cache::{TAPROOT_LOCK_SCRIPTS_CACHE, TAPROOT_SPEND_INFO_CACHE},
    },
    commitments::CommitmentMessageId,
    common::ZkProofVerifyingKey,
    connectors::base::*,
    error::{ChunkerError, ConnectorError, Error},
    transactions::base::Input,
    utils::{
        cleanup_cache_files, compress, decompress, read_disk_cache,
        remove_script_and_control_block_from_witness, write_disk_cache,
    },
};
use bitcoin::{
    hashes::{hash160, Hash},
    key::TweakedPublicKey,
    taproot::{ControlBlock, LeafVersion, TaprootBuilder, TaprootSpendInfo},
    Address, Network, ScriptBuf, TapNodeHash, Transaction, TxIn, XOnlyPublicKey,
};
use num_traits::ToPrimitive;
use secp256k1::SECP256K1;
use serde::{Deserialize, Serialize};

use bitvm::{
    chunker::{
        assigner::BridgeAssigner,
        chunk_groth16_verifier::groth16_verify_to_segments,
        common::RawWitness,
        disprove_execution::{disprove_exec, RawProof},
    },
    signatures::signing_winternitz::WinternitzPublicKey,
};
use zstd::DEFAULT_COMPRESSION_LEVEL;

// Specialized for assert leaves currently.
pub type LockScript = fn(index: u32) -> ScriptBuf;
pub type UnlockWitnessData = Vec<u8>;
pub type UnlockWitness = fn(index: u32) -> UnlockWitnessData;

pub struct DisproveLeaf {
    pub lock: LockScript,
    pub unlock: UnlockWitness,
}

const CACHE_DIRECTORY_NAME: &str = "cache";
const LOCK_SCRIPTS_FILE_PREFIX: &str = "lock_scripts_";
const MAX_CACHE_FILES: u32 = 90; //~1GB in total, based on lock scripts cache being 11MB each

fn get_lock_scripts_cache_path(cache_id: &str) -> PathBuf {
    let lock_scripts_file_name = format!("{LOCK_SCRIPTS_FILE_PREFIX}{}.bin", cache_id);
    Path::new(BRIDGE_DATA_DIRECTORY_NAME)
        .join(CACHE_DIRECTORY_NAME)
        .join(lock_scripts_file_name)
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct ConnectorC {
    pub network: Network,
    pub operator_taproot_public_key: XOnlyPublicKey,
    commitment_public_keys: BTreeMap<CommitmentMessageId, WinternitzPublicKey>,
}

impl ConnectorC {
    pub fn new(
        network: Network,
        operator_taproot_public_key: &XOnlyPublicKey,
        commitment_public_keys: &BTreeMap<CommitmentMessageId, WinternitzPublicKey>,
    ) -> Self {
        ConnectorC {
            network,
            operator_taproot_public_key: *operator_taproot_public_key,
            commitment_public_keys: commitment_public_keys.clone(),
        }
    }

    pub fn generate_disprove_witness(
        &self,
        commit_1_witness: Vec<RawWitness>,
        commit_2_witness: Vec<RawWitness>,
        vk: &ZkProofVerifyingKey,
    ) -> Result<(usize, RawWitness), Error> {
        let pks = self
            .commitment_public_keys
            .clone()
            .into_iter()
            .map(|(k, v)| {
                (
                    match k {
                        CommitmentMessageId::Groth16IntermediateValues((name, _)) => name,
                        _ => String::new(),
                    },
                    v,
                )
            })
            .collect();
        let mut assigner = BridgeAssigner::new_watcher(pks);
        // merge commit1 and commit2
        disprove_exec(
            &mut assigner,
            vec![commit_1_witness, commit_2_witness],
            vk.clone(),
        )
        .ok_or(Error::Chunker(ChunkerError::ValidProof))
    }

    pub fn taproot_merkle_root(&self) -> Option<TapNodeHash> {
        self.taproot_spend_info_cache()
            .map(|cache| cache.merkle_root)
            .unwrap_or_else(|| self.generate_taproot_spend_info().merkle_root())
    }

    pub fn taproot_output_key(&self) -> TweakedPublicKey {
        self.taproot_spend_info_cache()
            .map(|cache| cache.output_key)
            .unwrap_or_else(|| self.generate_taproot_spend_info().output_key())
    }

    pub fn taproot_scripts_len(&self) -> usize {
        self.taproot_spend_info_cache()
            .map(|cache| cache.scripts_length)
            .unwrap_or_else(|| self.generate_taproot_spend_info().script_map().len())
    }

    pub fn taproot_script_and_control_block(&self, leaf_index: usize) -> (ScriptBuf, ControlBlock) {
        self.lock_script_cache(leaf_index)
            .and_then(|cache| {
                decompress(&cache.encoded_script)
                    .ok()
                    .map(|data| (data, cache.control_block))
            })
            .and_then(|(encoded, control_block)| {
                bitcode::decode::<Vec<u8>>(&encoded)
                    .ok()
                    .map(|decoded| (ScriptBuf::from(decoded), control_block))
            })
            .unwrap_or_else(|| {
                generate_script_and_control_block(
                    &self.generate_taproot_spend_info(),
                    &self.lock_scripts_bytes(),
                    leaf_index,
                )
            })
    }

    // read from cache or generate from [`TaprootConnector`]
    fn taproot_spend_info_cache(&self) -> Option<TaprootSpendInfoCache> {
        match Self::spend_info_cache_id(&self.commitment_public_keys).map(|cache_id| {
            TAPROOT_SPEND_INFO_CACHE
                .write()
                .unwrap()
                .get(&cache_id)
                .cloned()
        }) {
            Ok(Some(cache)) => Some(cache),
            Ok(None) => {
                let spend_info = self.generate_taproot_spend_info();
                Some(TaprootSpendInfoCache::from(&spend_info))
            }
            _ => None,
        }
    }

    fn lock_scripts_bytes(&self) -> Vec<Vec<u8>> {
        match Self::spend_info_cache_id(&self.commitment_public_keys) {
            Ok(cache_id) => {
                let file_path = get_lock_scripts_cache_path(&cache_id);
                let lock_scripts_bytes = read_disk_cache(&file_path)
                    .inspect_err(|e| {
                        eprintln!(
                            "Failed to read lock scripts cache from expected location: {}",
                            e
                        );
                    })
                    .ok()
                    .unwrap_or_else(|| generate_assert_leaves(&self.commitment_public_keys));
                if !file_path.exists() {
                    write_disk_cache(&file_path, &lock_scripts_bytes)
                        .map_err(|e| format!("Failed to write lock scripts cache to disk: {}", e))
                        .unwrap();
                }
                cleanup_cache_files(
                    LOCK_SCRIPTS_FILE_PREFIX,
                    file_path.parent().unwrap(),
                    MAX_CACHE_FILES,
                );

                lock_scripts_bytes
            }
            _ => generate_assert_leaves(&self.commitment_public_keys),
        }
    }

    fn lock_script_cache(&self, leaf_index: usize) -> Option<LockScriptCache> {
        match Self::lock_script_cache_id(&self.commitment_public_keys, leaf_index).map(|cache_id| {
            (
                TAPROOT_LOCK_SCRIPTS_CACHE
                    .write()
                    .unwrap()
                    .get(&cache_id)
                    .cloned(),
                cache_id,
            )
        }) {
            Ok((Some(cache), _)) => Some(cache),
            Ok((None, cache_id)) => {
                let spend_info = self.generate_taproot_spend_info();
                let (script, control_block) = generate_script_and_control_block(
                    &spend_info,
                    &self.lock_scripts_bytes(),
                    leaf_index,
                );
                let encoded_data = bitcode::encode(script.as_bytes());
                let compressed_data = compress(&encoded_data, DEFAULT_COMPRESSION_LEVEL)
                    .expect("Unable to compress script for caching");

                let saved_cache = TAPROOT_LOCK_SCRIPTS_CACHE.write().unwrap().put(
                    cache_id,
                    LockScriptCache {
                        control_block,
                        encoded_script: compressed_data,
                    },
                );

                saved_cache
            }
            _ => None,
        }
    }

    fn spend_info_cache_id(
        commitment_public_keys: &BTreeMap<CommitmentMessageId, WinternitzPublicKey>,
    ) -> Result<String, ConnectorError> {
        let bytes = first_winternitz_public_key_bytes(commitment_public_keys)?;
        let hash = hash160::Hash::hash(&bytes);
        Ok(hex::encode(hash))
    }

    fn lock_script_cache_id(
        commitment_public_keys: &BTreeMap<CommitmentMessageId, WinternitzPublicKey>,
        leaf_index: usize,
    ) -> Result<String, ConnectorError> {
        let mut bytes = first_winternitz_public_key_bytes(commitment_public_keys)?;
        bytes.append(leaf_index.to_be_bytes().to_vec().as_mut());
        let hash = hash160::Hash::hash(&bytes);
        Ok(hex::encode(hash))
    }
}

impl TaprootConnector for ConnectorC {
    fn generate_taproot_leaf_script(&self, _: u32) -> ScriptBuf {
        // use taproot_script_and_control_block to return cached script and control block
        unreachable!("Cache is not used for leaf scripts");
    }

    fn generate_taproot_leaf_tx_in(&self, leaf_index: u32, input: &Input) -> TxIn {
        let index = leaf_index.to_usize().unwrap();
        if index >= self.taproot_scripts_len() {
            panic!("Invalid leaf index.")
        }
        generate_default_tx_in(input)
    }

    fn generate_taproot_spend_info(&self) -> TaprootSpendInfo {
        println!("Generating new taproot spend info for connector C...");
        let lock_script_bytes = self.lock_scripts_bytes();
        let script_weights = lock_script_bytes
            .iter()
            .map(|b| (1, ScriptBuf::from_bytes(b.clone())));

        let spend_info = TaprootBuilder::with_huffman_tree(script_weights)
            .expect("Unable to add assert leaves")
            .finalize(SECP256K1, self.operator_taproot_public_key)
            .expect("Unable to finalize assert transaction connector c taproot");

        // write to cache
        if let Ok(cache_id) = Self::spend_info_cache_id(&self.commitment_public_keys) {
            let spend_info_cache = TaprootSpendInfoCache::from(&spend_info);
            if !TAPROOT_SPEND_INFO_CACHE.read().unwrap().contains(&cache_id) {
                TAPROOT_SPEND_INFO_CACHE
                    .write()
                    .unwrap()
                    .push(cache_id, spend_info_cache);
            }
        }

        spend_info
    }

    fn generate_taproot_address(&self) -> Address {
        Address::p2tr_tweaked(self.taproot_output_key(), self.network)
    }
}

fn first_winternitz_public_key_bytes(
    commitment_public_keys: &BTreeMap<CommitmentMessageId, WinternitzPublicKey>,
) -> Result<Vec<u8>, ConnectorError> {
    let (_, first_winternitz_public_key) = commitment_public_keys
        .iter()
        .next()
        .ok_or(ConnectorError::ConnectorCCommitsPublicKeyEmpty)?;
    Ok(first_winternitz_public_key
        .public_key
        .as_flattened()
        .to_vec())
}

fn generate_script_and_control_block(
    spend_info: &TaprootSpendInfo,
    lock_scripts_bytes: &Vec<Vec<u8>>,
    leaf_index: usize,
) -> (ScriptBuf, ControlBlock) {
    let script = ScriptBuf::from(lock_scripts_bytes[leaf_index].clone());
    let prevout_leaf = (script, LeafVersion::TapScript);
    let control_block = spend_info
        .control_block(&prevout_leaf)
        .expect("Unable to create Control block");
    (prevout_leaf.0, control_block)
}

pub fn generate_assert_leaves(
    commits_public_keys: &BTreeMap<CommitmentMessageId, WinternitzPublicKey>,
) -> Vec<Vec<u8>> {
    println!("Generating new lock scripts...");
    // hash map to btree map
    let pks = commits_public_keys
        .clone()
        .into_iter()
        .map(|(k, v)| {
            (
                match k {
                    CommitmentMessageId::Groth16IntermediateValues((name, _)) => name,
                    _ => String::new(),
                },
                v,
            )
        })
        .collect();
    let mut bridge_assigner = BridgeAssigner::new_watcher(pks);
    let default_proof = RawProof::default(); // mock a default proof to generate scripts

    let segments = groth16_verify_to_segments(
        &mut bridge_assigner,
        &default_proof.public,
        &default_proof.proof,
        &default_proof.vk,
    );

    let mut locks = Vec::with_capacity(1000);
    for segment in segments {
        locks.push(segment.script(&bridge_assigner).compile().into_bytes());
    }
    locks
}

pub fn get_commit_from_assert_commit_tx(assert_commit_tx: &Transaction) -> Vec<RawWitness> {
    let mut assert_commit_witness = Vec::new();
    for input in assert_commit_tx.input.iter() {
        // remove script and control block from witness
        let witness = remove_script_and_control_block_from_witness(input.witness.to_vec());
        assert_commit_witness.push(witness);
    }

    assert_commit_witness
}
