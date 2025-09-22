pub mod circuit;
pub mod cmd;
pub mod utils;

use std::{fs::File, path::PathBuf};

use crate::{circuit::{proposal::Parameters, ConstraintF, Proposal}, cmd::Commands};

use ark_bls12_381::{Bls12_381, FrParameters};
use ark_ff::{BigInteger, BitIteratorBE, Field, Fp256, PrimeField, ToBytes, ToConstraintField};
use ark_groth16::Groth16;
use ark_relations::r1cs::ConstraintLayer;
use ark_serialize::{CanonicalSerialize};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use clap::Parser;
use rand::{thread_rng, RngCore};
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

    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let subscriber = Registry::default().with(ConstraintLayer::default());
    tracing::subscriber::set_global_default(subscriber).unwrap();
    // let cli = Cli::parse();
    // let url = cli.rpc_url.unwrap();
    // let response = reqwest::get(url).await?.text().await?;
    // let data = response.split(",");

    let mut rng = StdRng::from_rng(thread_rng()).unwrap();

    let n_voters = 10;
    let mut data = String::from("");
    let mut voting_keys = vec![]; 
    for _ in 0..(n_voters-1) {
        let mut bytes = [0u8;32];
        rng.fill_bytes(&mut bytes);

        voting_keys.push(bytes.to_vec());
        let encoded = hex::encode(bytes);
        data.push_str(&encoded);
        data.push(',');
    }
    let parameters = Parameters::init(&mut rng); 

    let prop = Proposal::<Groth16<Bls12_381>>::new(parameters.clone());


    // let voters: Vec<Vec<u8>>  = data.map(|voter| {
    //     let bytes = hex::decode(voter).unwrap();
    //     bytes
    // }).collect();

    // let n_voters : usize = voters.len();
    let mut tree = prop.new_tree(8).unwrap();
    

    let voter = prop.new_voter(&mut rng);
    
    let base_prime_field = BitIteratorBE::new(ConstraintF::characteristic());

    let mut bits: Vec<bool> = BitIteratorBE::new(voter.voting_key.into_repr())
        .zip(base_prime_field)
        .skip_while(| (_, c) | !c)
        .map(|(b, _)| b)
        .collect();
    bits.reverse();
    let bytes: Vec<u8> = bits
        .chunks(8)
        .map(|chunk| {
            let mut val = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit {
                    val += 1<< i;
                }
            }
            val
        })
        .collect();
    let mut voting_key = [0u8; 32];
    
    voting_key.copy_from_slice(&bytes);
    
    let encoded = hex::encode(voting_key);
    data.push_str(&encoded);

    voting_keys.push(voting_key.to_vec());

    let mut idx: usize = 0;
    voting_keys.iter().enumerate().for_each(|(index, key)| {
        tree.update(index, key).unwrap();
        if key.eq(&voting_key) {
            idx = index;
        }
    });

    let proposal_id = prop.new_proposal_id(1 as u16);
    let nullifier = voter.nullifier(proposal_id);

    let vote = prop.new_vote(0u8);

    let proof = tree.generate_proof(idx).unwrap();
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

    
    // TODO: handle cli
    Ok(())
}


