pub mod circuit;
pub mod cmd;

use std::path::PathBuf;

use crate::{circuit::{proposal::Parameters, Proposal}, cmd::Commands};

use ark_bls12_381::Bls12_381;
use ark_groth16::Groth16;
use clap::Parser;
use reqwest;


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
    let cli = Cli::parse();
    let url = cli.rpc_url.unwrap();
    let response = reqwest::get(url).await?.text().await?;
    let data = response.split(",");


    let mut rng = ark_std::test_rng();
    let parameters = Parameters::init(&mut rng); 

    let prop = Proposal::<Groth16<Bls12_381>>::new(parameters.clone());

    let voters: Vec<Vec<u8>>  = data.map(|voter| {
        let bytes = hex::decode(voter).unwrap();
        bytes
    }).collect();

    let n_voters : usize = voters.len();
    let mut tree = prop.new_tree(n_voters).unwrap();
    let mut idx: usize = 0;
    voters.iter().for_each(|voter| {
        tree.update(idx, voter).unwrap();
        idx += 1;
    });

    // TODO: handle command and prove
    Ok(())
}


