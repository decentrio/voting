pub mod circuit;
pub mod cmd;
pub mod utils;

use std::{fs::File, path::PathBuf};

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
use gov::{CreateGroupRequest, ProposalRequest};
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
                    // let url = cli.rpc_url.unwrap();
                    // let response = reqwest::get(url).await?.text().await?;
                    let data = String::from("3bcc746344f900c1d08bb57f707010709a1ae626ec7d11391e66a338de51bf62,55af9fb47e387e6247d8d644f9fe4f84d6c61ead034eed1021c8dd370ad2a9af,12f0010135255cb45aebc559c3d9a486d8163fcec5d5b741b119ace4bac3a6ae,c5751f51351f4a3281e54f1235de8f55ff0193a64cf91ceab5288b337ada5840,32f3205594b1bd3e53514843378a5eb6934de4052ac09ab327255be5d5fdb319,aa6b2455a1626b6368044cc035c6ea2e39ce4701bc72cc4fbe2c548bac0652da,e66a52f5c542bfdab783483a556f657df2f2dacafeca87a763eb42a15fe14ceb,38448a379a9f8ed58e48e9799e03045433120b816a50b93409dd3368dcf1cb2c,1bd2e13b237f2381682a2f43a3e2551b194a639e42ffff1c1462c6a53605be4c");
                    let data =data.split(",");

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
            
            while true {
                let admin = prop.new_voter(&mut rng);
                let admin_pk = hex::encode(StoredKeypair::from(admin.voting_key.into_repr()).0);

                let n_members: usize = rng.gen_range(5..100);
                let n_proposals: usize = rng.gen_range(1..10);
                println!("n_members: {}", n_members);
                println!("n_proposal: {}", n_proposal);
                let mut members = vec![];
                let mut members_pk = vec![];
                for _ in 0..n_members {
                    let member = prop.new_voter(&mut rng);
                    members.push(member.clone());
                    members_pk.push(StoredKeypair::from(member.voting_key.into_repr()).0);
                }
                
                let request = tonic::Request::new(CreateGroupRequest {
                    admin: admin_pk,
                    threshold: (2 * n_members/ 3) as u64,
                    members: members_pk.iter().map(|key| hex::encode(key)).collect(),
                });

                let response = client.create_group(request).await.unwrap();
                println!("Group created with ID: {:?}", response.into_inner().group_id);

                let group_id = response.into_inner().group_id;

                for _ in 0..n_proposals {
                    // TODO: create proposal and vote
                }
            }
        }
    }
    Ok(())
}


