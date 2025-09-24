pub mod circuit;
pub mod cmd;
pub mod utils;

use std::{env, fs::File, path::PathBuf, time::{Duration, SystemTime, UNIX_EPOCH}};

use crate::{circuit::{proposal::Parameters, voter::Voter, Proposal}, cmd::{key::{KeyCommands, KeyConfig, StoredKeypair}, vote::VoteCommands, Commands}};

use ark_bls12_381::{Bls12_381};
use ark_ff::PrimeField;
use ark_groth16::Groth16;
use ark_relations::r1cs::ConstraintLayer;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use clap::Parser;
use rand::Rng;
use reqwest;
use tracing_subscriber::{layer::SubscriberExt, Registry};

pub mod gov {
    tonic::include_proto!("gov");
}
use gov::governance_client::GovernanceClient;
use gov::{CreateGroupRequest, ProposalRequest, VoteRequest, VoteOption};

use serde::Deserialize;
use dotenvy::dotenv;
use async_openai::{config::OpenAIConfig, types::{ChatCompletionRequestMessage, ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs}, Client};
#[derive(Debug, Deserialize)]
struct GPTProposal {
    title: String,
    description: String,
}

#[derive(Parser)]
#[command(author, version, about)]
#[command(name = "voting")]
#[command(about = "A simple CLI to interact with Veil protocol")]
pub(crate) struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short,long)]
    name: Option<String>,

    /// RPC url for fetching tree
    #[arg(short, long)]
    rpc_url: Option<String>,

    /// program id
    #[arg(short, long)]
    program_id: Option<String>,

    #[arg(short, long)]
    seed: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let subscriber = Registry::default().with(ConstraintLayer::default());
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let cli = Cli::parse();

    let mut rng : StdRng;
    if cli.seed.is_none() {
        rng =StdRng::from_entropy();
    } else {
        let seed = hex::decode(cli.seed.unwrap()).unwrap();
        let mut seed_bytes = [0u8;32];
        seed_bytes.copy_from_slice(&seed);
        rng = StdRng::from_seed(seed_bytes);
    }

    let parameters = Parameters::init(&mut rng); 
    let prop = Proposal::<Groth16<Bls12_381>>::new(parameters.clone());

    let voter =if cli.config.is_none() || cli.name.is_none() {
        prop.new_voter(&mut rng)
    } else {
        let mut key_path = cli.config.clone().unwrap();
        let name = cli.name.clone().unwrap();
        key_path.push(name);
        
        let keypair = StoredKeypair(std::fs::read(key_path).unwrap());
        Voter::from_keypair(&parameters.leaf_crh_params, keypair)
    };
    match cli.command {
        Commands::Key { command } => {
            let config = KeyConfig{
                path: cli.config.unwrap(),
                voter: Some(voter),
                name: cli.name,
                parameter: parameters
            };
            KeyCommands::handle_command(command, config).unwrap();
        },
        Commands::Vote { command } => {
            match command {
                VoteCommands::Vote { proposal_id, voter_index, vote_data } => {
                    let url = cli.rpc_url.unwrap();
                    let response = reqwest::get(url).await?.text().await?;
                    let data = response.split(",");

                    let voters: Vec<Vec<u8>>  = data.map(|voter| {
                        let bytes = hex::decode(voter).unwrap();
                        bytes
                    }).collect();

                    let mut tree = prop.new_tree(16).unwrap();
                    
                    let voting_key = StoredKeypair::from(voter.voting_key.into_repr());
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
        },
        Commands::Spam => {
            let mut client = GovernanceClient::connect("http://0.0.0.0:50051").await.unwrap();
            dotenv().ok();

            let api_key = env::var("OPENAI_API_KEY")
                .expect("OPENAI_API_KEY must be set in .env file");
            
            let config = OpenAIConfig::new().with_api_key(api_key);
            let gpt_client = Client::with_config(config);
            let prompt = r#"
                I want to generate a proposal in a range of 100-500 words for voting with 2 random options.
                The result data must only contain the following data in json like the example below, no further explanation,
                with description containing the 2 options:
                {"title":"....", "description":"...."}
            "#;

            
            // while true {
                let prop = Proposal::<Groth16<Bls12_381>>::new(parameters.clone());
                let admin = prop.new_voter(&mut rng);
                let admin_pk = hex::encode(StoredKeypair::from(admin.voting_key.into_repr()).0);

                let n_members: usize = rng.gen_range(5..100);
                let n_proposals: usize = rng.gen_range(1..10);
                println!("n_members: {}", n_members);
                println!("n_proposal: {}", n_proposals);
                let mut members = vec![];
                let mut members_pk = vec![];
                let mut tree = prop.new_tree(8).unwrap();
                let mut proofs = vec![];
                for i in 0..n_members {
                    let member = prop.new_voter(&mut rng);
                    members.push(member.clone());
                    let pk = StoredKeypair::from(member.voting_key.into_repr()).0;
                    members_pk.push(pk.clone());
                    tree.update(i, &pk).unwrap();
                    
                }

                for i in 0..n_members {
                    proofs.push(tree.generate_proof(i).unwrap());
                }
                let root = tree.root();
                println!("requesting create group");
                let request = tonic::Request::new(CreateGroupRequest {
                    admin: admin_pk,
                    threshold: (2 * n_members/ 3) as u64,
                    members: members_pk.iter().map(|key| hex::encode(key)).collect(),
                });

                let response = client.create_group(request).await.unwrap();
                let group_id: u64 = response.into_inner().group_id;
                println!("Group created with ID: {:?}", group_id);


                for _ in 0..n_proposals {
                    let gpt_request = CreateChatCompletionRequestArgs::default()
                        .model("gpt-4o-mini")
                        .messages([ChatCompletionRequestMessage::User(
                            ChatCompletionRequestUserMessage{
                                content: ChatCompletionRequestUserMessageContent::Text(String::from(prompt)),
                                name: None
                            }
                        )])
                        .build().unwrap();
                    let response = gpt_client.chat().create(gpt_request).await.unwrap();
                    let reply = response
                        .choices
                        .get(0)
                        .and_then(|c| c.message.content.as_ref())
                        .unwrap();
                    println!("gpt reply: {}", reply.clone());
                    let proposal: GPTProposal = serde_json::from_str(reply).unwrap();

                    let end_time = SystemTime::now().checked_add(Duration::from_secs(3600)).unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
                    println!("requesting create proposal");
                    let request = tonic::Request::new(ProposalRequest{
                        group_id, 
                        title: proposal.title,
                        description: proposal.description,
                        end_time
                    });

                    let response = client.submit_proposal(request).await.unwrap();
                    let prop_id: u64 = response.into_inner().proposal_id;
                    println!("Proposal submitted with ID: {}", prop_id);
                    let prop = Proposal::<Groth16<Bls12_381>>::new(parameters.clone());
                    let proposal_id = prop.new_proposal_id(prop_id as u16);
                    while !members.is_empty() {
                        let idx = rng.gen_range(0..members.len());
                        let voter = members.remove(idx);
                        let nullifier = voter.nullifier(proposal_id);
                        
                        let vote_data = if rng.gen_bool(0.5) { 1 } else { 0 } ;
                        let vote = prop.new_vote(vote_data);

                        let prop = Proposal::<Groth16<Bls12_381>>::new(parameters.clone());
                        let circuit = prop.new_circuit_instance(root, proposal_id, nullifier, vote, voter.sk, proofs[idx].clone());
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
                        
                        let vote_option = if vote_data == 1 {
                            VoteOption::Yes
                        } else {
                            VoteOption::No
                        };
                        println!("requesting submit vote");

                        let response = client.submit_vote(tonic::Request::new(VoteRequest {
                            group_id,
                            proposal_id: prop_id as u64,
                            option: vote_option as i32,
                            nullifier: StoredKeypair::from(nullifier.into_repr()).0,
                        })).await.unwrap();
                        println!("Vote response {:?}", response);
                    }
                }
            // }
        }
    }
    Ok(())
}


