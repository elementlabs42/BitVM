use super::key_command::KeysCommand;
use crate::client::client::BitVMClient;
use crate::common::ZkProofVerifyingKey;
use crate::constants::DestinationNetwork;
use crate::contexts::base::generate_keys_from_secret;
use crate::graphs::base::{VERIFIER_0_SECRET, VERIFIER_1_SECRET};
use crate::proof::{get_proof, ProofType};
use crate::transactions::base::Input;
use ark_serialize::CanonicalDeserialize;

use bitcoin::PublicKey;
use bitcoin::{Network, OutPoint};
use clap::{arg, ArgMatches, Command};
use colored::Colorize;
use std::collections::HashMap;
use std::io::{self, Write};
use std::str::FromStr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::sleep;

pub struct CommonArgs {
    pub key_dir: Option<String>,
    pub verifiers: Option<Vec<PublicKey>>,
    pub environment: Option<String>,
    pub path_prefix: Option<String>,
}

pub struct ClientCommand {
    client: BitVMClient,
    proof_queue: mpsc::Sender<(String, ProofType)>,
    proof_receiver: mpsc::Receiver<(String, ProofType)>,
    graph_states: HashMap<String, GraphState>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphState {
    Created,
    DepositConfirmed,
    AssertionMade(ProofType),
    AssertionDisputed,
    Completed,
    Failed,
}

impl ClientCommand {
    pub async fn new(common_args: CommonArgs) -> Self {
        let (source_network, destination_network) = match common_args.environment.as_deref() {
            Some("mainnet") => (Network::Bitcoin, DestinationNetwork::Ethereum),
            Some("testnet") => (Network::Testnet, DestinationNetwork::EthereumSepolia),
            _ => {
                eprintln!("Invalid environment. Use mainnet, testnet.");
                std::process::exit(1);
            }
        };

        let keys_command = KeysCommand::new(common_args.key_dir);
        let config = keys_command.read_config().expect("Failed to read config");

        let n_of_n_public_keys = common_args.verifiers.unwrap_or_else(|| {
            let (_, verifier_0_public_key) =
                generate_keys_from_secret(source_network, VERIFIER_0_SECRET);
            let (_, verifier_1_public_key) =
                generate_keys_from_secret(source_network, VERIFIER_1_SECRET);
            vec![verifier_0_public_key, verifier_1_public_key]
        });

        let mut verifying_key = None;
        if let Some(vk) = config.keys.verifying_key {
            let bytes = hex::decode(vk).unwrap();
            verifying_key = Some(ZkProofVerifyingKey::deserialize_compressed(&*bytes).unwrap());
        }

        let bitvm_client = BitVMClient::new(
            None,
            source_network,
            destination_network,
            &n_of_n_public_keys,
            config.keys.depositor.as_deref(),
            config.keys.operator.as_deref(),
            config.keys.verifier.as_deref(),
            config.keys.withdrawer.as_deref(),
            common_args.path_prefix.as_deref(),
            verifying_key,
        )
        .await;

        let (tx, rx) = mpsc::channel(32);

        Self {
            client: bitvm_client,
            proof_queue: tx,
            proof_receiver: rx,
            graph_states: HashMap::new(),
        }
    }

    pub fn get_depositor_address_command() -> Command {
        Command::new("get-depositor-address")
            .short_flag('d')
            .about("Get an address spendable by the registered depositor key")
            .after_help("Get an address spendable by the registered depositor key")
    }

    pub async fn handle_get_depositor_address(&mut self) -> io::Result<()> {
        let address = self.client.get_depositor_address().to_string();
        println!("{address}");
        Ok(())
    }

    pub fn get_depositor_utxos_command() -> Command {
        Command::new("get-depositor-utxos")
            .short_flag('u')
            .about("Get a list of the depositor's utxos")
            .after_help("Get a list of the depositor's utxos")
    }

    pub async fn handle_get_depositor_utxos(&mut self) -> io::Result<()> {
        for utxo in self.client.get_depositor_utxos().await {
            println!("{}:{} {}", utxo.txid, utxo.vout, utxo.value);
        }
        Ok(())
    }

    pub fn get_initiate_peg_in_command() -> Command {
        Command::new("initiate-peg-in")
        .short_flag('n')
        .about("Initiate a peg-in")
        .after_help("Initiate a peg-in by creating a peg-in graph")
        .arg(arg!(-u --utxo <UTXO> "Specify the uxo to spend from. Format: <TXID>:<VOUT>")
        .required(true))
        .arg(arg!(-d --destination_address <EVM_ADDRESS> "The evm-address to send the wrapped bitcoin to")
            .required(true))
    }

    pub async fn handle_initiate_peg_in_command(
        &mut self,
        sub_matches: &ArgMatches,
    ) -> io::Result<()> {
        let utxo = sub_matches.get_one::<String>("utxo").unwrap();
        let evm_address = sub_matches
            .get_one::<String>("destination_address")
            .unwrap();
        let outpoint = OutPoint::from_str(utxo).unwrap();

        let tx = self.client.esplora.get_tx(&outpoint.txid).await.unwrap();
        let tx = tx.unwrap();
        let input = Input {
            outpoint,
            amount: tx.output[outpoint.vout as usize].value,
        };
        let peg_in_id = self.client.create_peg_in_graph(input, evm_address).await;

        self.client.flush().await;

        println!("Created peg-in with ID {peg_in_id}. Broadcasting deposit...");

        match self.client.broadcast_peg_in_deposit(&peg_in_id).await {
            Ok(txid) => println!("Broadcasted peg-in deposit with txid {txid}"),
            Err(e) => println!("Failed to broadcast peg-in deposit: {}", e),
        }
        Ok(())
    }

    pub fn get_automatic_command() -> Command {
        Command::new("automatic")
            .short_flag('a')
            .about("Automatic mode: Poll for status updates and sign or broadcast transactions")
    }

    pub async fn handle_automatic_assertion(
        &mut self,
        graph_id: &str,
        proof: ProofType,
    ) -> io::Result<()> {
        self.graph_states.insert(
            graph_id.to_string(),
            GraphState::AssertionMade(proof.clone()),
        );

        match proof {
            ProofType::Commit1 => {
                self.client
                    .broadcast_assert_commit_1(graph_id, &proof)
                    .await?;
            }
            ProofType::Commit2 => {
                self.client
                    .broadcast_assert_commit_2(graph_id, &proof)
                    .await?;
            }
            ProofType::Final => {
                self.client.broadcast_assert_final(graph_id).await?;
            }
        }

        self.proof_queue
            .send((graph_id.to_string(), proof))
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(())
    }

    pub async fn handle_automatic_verification(&mut self) -> io::Result<()> {
        while let Some((graph_id, proof)) = self.proof_receiver.recv().await {
            let is_valid = self.verify_proof(&proof).await;

            if !is_valid {
                match proof {
                    ProofType::Commit1 => {
                        self.client.broadcast_take_1(&graph_id).await?;
                    }
                    ProofType::Commit2 => {
                        self.client.broadcast_take_2(&graph_id).await?;
                    }
                    ProofType::Final => {
                        self.client.broadcast_take_final(&graph_id).await?;
                    }
                }

                self.graph_states
                    .insert(graph_id, GraphState::AssertionDisputed);
            } else {
                self.graph_states.insert(graph_id, GraphState::Completed);
            }
        }
        Ok(())
    }

    async fn verify_proof(&self, proof: &ProofType) -> bool {
        true
    }

    pub async fn handle_automatic_command(&mut self) -> io::Result<()> {
        let verification_handle = tokio::spawn(self.handle_automatic_verification());

        loop {
            self.client.sync().await;

            let old_data = self.client.data().clone();

            self.client.process_peg_ins().await;
            self.client.process_peg_outs().await;

            for (graph_id, state) in self.graph_states.clone().iter() {
                match state {
                    GraphState::Created => {
                        if self.client.is_ready_for_assertion(graph_id).await {
                            self.handle_automatic_assertion(graph_id, ProofType::Commit1)
                                .await?;
                        }
                    }
                    GraphState::AssertionMade(proof) => {
                        if self.client.is_assertion_disputed(graph_id).await {
                            self.graph_states
                                .insert(graph_id.clone(), GraphState::AssertionDisputed);
                        }
                    }
                    _ => {}
                }
            }

            if self.client.data() != &old_data {
                self.client.flush().await;
            } else {
                sleep(Duration::from_millis(250)).await;
            }
        }

        verification_handle.abort();
        Ok(())
    }

    pub fn get_broadcast_command() -> Command {
        Command::new("broadcast")
            .short_flag('b')
            .about("Broadcast transactions")
            .after_help("Broadcast transactions.")
            .subcommand(
                Command::new("pegin")
                    .about("Broadcast peg-in transactions")
                    .arg(arg!(-g --graph_id <GRAPH_ID> "Peg-in graph ID").required(true))
                    .subcommand(Command::new("deposit").about("Broadcast peg-in deposit"))
                    .subcommand(Command::new("refund").about("Broadcast peg-in refund"))
                    .subcommand(Command::new("confirm").about("Broadcast peg-in confirm"))
                    .subcommand_required(true),
            )
            .subcommand(
                Command::new("tx")
                    .about("Broadcast transactions")
                    .arg(arg!(-g --graph_id <GRAPH_ID> "Peg-out graph ID").required(true))
                    .subcommand(Command::new("peg_out_confirm").about("Broadcast peg-out confirm"))
                    .subcommand(Command::new("kick_off_1").about("Broadcast kick off 1"))
                    .subcommand(Command::new("kick_off_2").about("Broadcast kick off 2"))
                    .subcommand(Command::new("start_time").about("Broadcast start time"))
                    .subcommand(Command::new("assert_initial").about("Broadcast assert initial"))
                    .subcommand(
                        Command::new("assert_commit_1").about("Broadcast assert commitment 1"),
                    )
                    .subcommand(
                        Command::new("assert_commit_2").about("Broadcast assert commitment 2"),
                    )
                    .subcommand(Command::new("assert_final").about("Broadcast assert final"))
                    .subcommand(Command::new("take_1").about("Broadcast take 1"))
                    .subcommand(Command::new("take_2").about("Broadcast take 2"))
                    .subcommand_required(true),
            )
            .subcommand_required(true)
    }

    pub async fn handle_broadcast_command(&mut self, sub_matches: &ArgMatches) -> io::Result<()> {
        let subcommand = sub_matches.subcommand();
        let graph_id = subcommand.unwrap().1.get_one::<String>("graph_id").unwrap();

        let result = match subcommand.unwrap().1.subcommand() {
            Some(("deposit", _)) => self.client.broadcast_peg_in_deposit(graph_id).await,
            Some(("refund", _)) => self.client.broadcast_peg_in_refund(graph_id).await,
            Some(("confirm", _)) => self.client.broadcast_peg_in_confirm(graph_id).await,
            Some(("peg_out_confirm", _)) => self.client.broadcast_peg_out_confirm(graph_id).await,
            Some(("kick_off_1", _)) => self.client.broadcast_kick_off_1(graph_id).await,
            Some(("kick_off_2", _)) => self.client.broadcast_kick_off_2(graph_id).await,
            Some(("start_time", _)) => self.client.broadcast_start_time(graph_id).await,
            Some(("assert_initial", _)) => self.client.broadcast_assert_initial(graph_id).await,
            Some(("assert_commit_1", _)) => {
                self.client
                    .broadcast_assert_commit_1(graph_id, &get_proof())
                    .await
            }
            Some(("assert_commit_2", _)) => {
                self.client
                    .broadcast_assert_commit_2(graph_id, &get_proof())
                    .await
            }
            Some(("assert_final", _)) => self.client.broadcast_assert_final(graph_id).await,
            Some(("take_1", _)) => self.client.broadcast_take_1(graph_id).await,
            Some(("take_2", _)) => self.client.broadcast_take_2(graph_id).await,
            _ => unreachable!(),
        };

        match result {
            Ok(txid) => println!("Broadcasted transaction with txid {txid}"),
            Err(e) => println!("Failed to broadcast transaction: {}", e),
        }

        Ok(())
    }

    pub fn get_status_command() -> Command {
        Command::new("status")
            .short_flag('s')
            .about("Show the status of the BitVM client")
            .after_help("Get the status of the BitVM client.")
    }

    pub async fn handle_status_command(&mut self) -> io::Result<()> {
        self.client.sync().await;
        self.client.status().await;
        Ok(())
    }

    pub fn get_interactive_command() -> Command {
        Command::new("interactive")
            .short_flag('i')
            .about("Interactive mode for manually issuing commands")
    }

    pub fn get_automatic_assert_command() -> Command {
        Command::new("automatic-assert")
            .short_flag('t')
            .about("Automatic assertion mode: Automatically make assertions for testing")
            .arg(arg!(-g --graph_id <GRAPH_ID> "Graph ID to make assertions for").required(true))
            .arg(
                arg!(-m --mode <MODE> "Assertion mode: valid or invalid")
                    .value_parser(["valid", "invalid"])
                    .default_value("valid")
                    .required(false),
            )
    }

    pub fn get_automatic_verify_command() -> Command {
        Command::new("automatic-verify")
            .short_flag('v')
            .about("Automatic verification mode: Verify assertions from other clients")
            .arg(arg!(-g --graph_id <GRAPH_ID> "Graph ID to verify assertions for").required(true))
    }

    pub async fn handle_interactive_command(&mut self, main_command: &Command) -> io::Result<()> {
        println!(
            "{}",
            "Entering interactive mode. Type 'help' for a list of commands and 'exit' to quit."
                .green()
        );

        let mut stdin_reader = BufReader::new(tokio::io::stdin());
        loop {
            print!("{}", "bitvm >> ".bold());
            io::stdout().flush().unwrap(); // Ensure the prompt is printed out immediately

            let mut line = String::new();
            stdin_reader.read_line(&mut line).await.unwrap();
            let input = line.trim();

            if input == "exit" {
                break;
            }

            let mut args = vec!["bitvm"];
            args.extend(input.split_whitespace());

            let matches = match main_command.clone().try_get_matches_from(args) {
                Ok(matches) => matches,
                Err(e) => {
                    if !e.to_string().to_lowercase().contains("error") {
                        println!("{}", format!("{}", e).green());
                    } else {
                        println!("{}", format!("{}", e).red());
                    }
                    continue;
                }
            };

            if let Some(sub_matches) = matches.subcommand_matches("keys") {
                let key_dir = matches.get_one::<String>("key-dir").cloned();
                let keys_command = KeysCommand::new(key_dir);
                keys_command.handle_command(sub_matches)?;
            } else if matches
                .subcommand_matches("get-depositor-address")
                .is_some()
            {
                self.handle_get_depositor_address().await?;
            } else if matches.subcommand_matches("get-depositor-utxos").is_some() {
                self.handle_get_depositor_utxos().await?;
            } else if let Some(sub_matches) = matches.subcommand_matches("initiate-peg-in") {
                self.handle_initiate_peg_in_command(sub_matches).await?;
            } else if matches.subcommand_matches("status").is_some() {
                self.handle_status_command().await?;
            } else if let Some(sub_matches) = matches.subcommand_matches("broadcast") {
                self.handle_broadcast_command(sub_matches).await?;
            } else if let Some(sub_matches) = matches.subcommand_matches("automatic-assert") {
                self.handle_automatic_assert_command(sub_matches).await?;
            } else if let Some(sub_matches) = matches.subcommand_matches("automatic-verify") {
                self.handle_automatic_verify_command(sub_matches).await?;
            } else if matches.subcommand_matches("automatic").is_some() {
                self.handle_automatic_command().await?;
            } else if matches.subcommand_matches("interactive").is_some() {
                println!("{}", "Already in interactive mode.".yellow());
            } else {
                println!(
                    "{}",
                    "Unknown command. Type 'help' for a list of commands.".red()
                );
            }
        }

        println!("{}", "Exiting interactive mode.".green());
        Ok(())
    }

    pub async fn handle_automatic_assert_command(
        &mut self,
        sub_matches: &ArgMatches,
    ) -> io::Result<()> {
        let graph_id = sub_matches.get_one::<String>("graph_id").unwrap();
        let mode = sub_matches.get_one::<String>("mode").unwrap();
        let is_valid = mode == "valid";

        println!("Starting automatic assertion mode for graph {}", graph_id);
        println!(
            "Making {} assertions...",
            if is_valid { "valid" } else { "invalid" }
        );

        // Set initial state
        self.graph_states
            .insert(graph_id.to_string(), GraphState::Created);

        loop {
            self.client.sync().await;

            match self.graph_states.get(graph_id) {
                Some(GraphState::Created) => {
                    if self.client.is_ready_for_assertion(graph_id).await {
                        let proof = if is_valid {
                            ProofType::Commit1 // Use valid proof
                        } else {
                            ProofType::Commit1 // TODO: Generate invalid proof
                        };
                        self.handle_automatic_assertion(graph_id, proof).await?;
                    }
                }
                Some(GraphState::AssertionMade(ProofType::Commit1)) => {
                    if !self.client.is_assertion_disputed(graph_id).await {
                        let proof = if is_valid {
                            ProofType::Commit2
                        } else {
                            ProofType::Commit2 // TODO: Generate invalid proof
                        };
                        self.handle_automatic_assertion(graph_id, proof).await?;
                    }
                }
                Some(GraphState::AssertionMade(ProofType::Commit2)) => {
                    if !self.client.is_assertion_disputed(graph_id).await {
                        let proof = if is_valid {
                            ProofType::Final
                        } else {
                            ProofType::Final // TODO: Generate invalid proof
                        };
                        self.handle_automatic_assertion(graph_id, proof).await?;
                    }
                }
                Some(GraphState::Completed) | Some(GraphState::Failed) => {
                    println!(
                        "Assertion sequence completed with state: {:?}",
                        self.graph_states.get(graph_id).unwrap()
                    );
                    break;
                }
                _ => {}
            }

            sleep(Duration::from_millis(250)).await;
        }

        Ok(())
    }

    pub async fn handle_automatic_verify_command(
        &mut self,
        sub_matches: &ArgMatches,
    ) -> io::Result<()> {
        let graph_id = sub_matches.get_one::<String>("graph_id").unwrap();

        println!(
            "Starting automatic verification mode for graph {}",
            graph_id
        );
        println!("Waiting for assertions to verify...");

        // Start verification loop
        let verification_handle = tokio::spawn(self.handle_automatic_verification());

        loop {
            self.client.sync().await;

            match self.graph_states.get(graph_id) {
                Some(GraphState::Completed) | Some(GraphState::Failed) => {
                    println!(
                        "Verification completed with state: {:?}",
                        self.graph_states.get(graph_id).unwrap()
                    );
                    break;
                }
                _ => {}
            }

            sleep(Duration::from_millis(250)).await;
        }

        verification_handle.abort();
        Ok(())
    }
}
