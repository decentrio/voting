pub mod circuit;
pub mod cmd;
pub mod utils;

use std::{fs::File, path::PathBuf};

use crate::{circuit::{proposal::Parameters, Proposal}, cmd::{key::{KeyCommands, KeyConfig, StoredKeypair}, vote::VoteCommands, Commands}};

use ark_bls12_381::{Bls12_381};
use ark_groth16::Groth16;
use ark_relations::r1cs::ConstraintLayer;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use clap::Parser;
use reqwest;
use tracing_subscriber::{layer::SubscriberExt, Registry};


#[derive(Parser)]
#[command(author, version, about)]
#[command(name = "voting")]
#[command(about = "A simple CLI to interact with Veil protocol")]
pub(crate) struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// RPC url for fetching tree
    #[arg(short, long)]
    rpc_url: Option<String>,

    /// program id
    #[arg(short, long)]
    program_id: String,

    #[arg(short, long)]
    seed: String,

    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let subscriber = Registry::default().with(ConstraintLayer::default());
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let cli = Cli::parse();

    let mut rng : StdRng;
    let seed = hex::decode(cli.seed).unwrap();
    if seed.len() == 0 {
        rng =StdRng::from_entropy();
    } else {
         let mut seed_bytes = [0u8;32];
        seed_bytes.copy_from_slice(&seed);
        rng = StdRng::from_seed(seed_bytes);
    }

    let parameters = Parameters::init(&mut rng); 
    let prop = Proposal::<Groth16<Bls12_381>>::new(parameters.clone());
    let voter = prop.new_voter(&mut rng);
    match cli.command {
        Commands::Key { command } => {
            // TODO: parse path
            let config = KeyConfig{
                path: cli.config.unwrap(),
                voter: Some(voter),
            };
            KeyCommands::handle_command(command, config).unwrap();
        },
        Commands::Vote { command } => {
            match command {
                VoteCommands::Vote { key_path, proposal_id, voter_index, vote_data } => {
                    let url = cli.rpc_url.unwrap();
                    let response = reqwest::get(url).await?.text().await?;
                    let data = response.split(",");

                    let voters: Vec<Vec<u8>>  = data.map(|voter| {
                        let bytes = hex::decode(voter).unwrap();
                        bytes
                    }).collect();

                    let mut tree = prop.new_tree(16).unwrap();
                    
                    let voting_key = StoredKeypair::from(voter.voting_key.0);
                    voters.iter().enumerate().for_each(|(index, key)| {
                        if index == voter_index {
                            if !key.eq(&voting_key.0) {
                                panic!("invalid pubkey");
                            }
                        }
                        tree.update(index, key).unwrap();
                    });

                    let proposal_id = prop.new_proposal_id(proposal_id);
                    let nullifier = voter.nullifier(proposal_id);

                    let vote = prop.new_vote(vote_data);

                    let proof = tree.generate_proof(voter_index).unwrap();
                    let root = tree.root();

                    let circuit = prop.new_circuit_instance(root, proposal_id, nullifier, vote, voter.sk, proof);
                    let (pk, vk) =
                            Proposal::<Groth16<Bls12_381>>::circuit_setup(&mut rng, circuit.clone()).unwrap();
                    
                    let vk = vk;
                    
                    let public_inputs = circuit.clone().public_inputs();
                    let public_inputs_json = utils::public_inputs_to_snarkjs(&public_inputs);
                    let mut writer = File::create("./data/public_inputs.json").unwrap();
                    serde_json::to_writer_pretty(writer, &public_inputs_json).unwrap();
                    
                    let vk_json = utils::vk_to_snarkjs(&vk, public_inputs.len()).unwrap();
                    writer = File::create("./data/vkey.json").unwrap();
                    serde_json::to_writer_pretty(writer, &vk_json).unwrap();

                    let proof = Proposal::<Groth16<Bls12_381>>::prove(&mut rng, pk, circuit).unwrap();

                    writer = File::create("./data/proof.json").unwrap();
                    let proof_json = utils::proof_to_snarkjs(&proof);
                    serde_json::to_writer_pretty(writer, &proof_json).unwrap();
                }
            }
        }
    }
    Ok(())
}


