use std::path::PathBuf;

use clap::Subcommand;

#[derive(Clone, Subcommand)]
pub enum VoteCommands {
    Vote{
        #[arg(short, long)]
        proposal_id: u16,

        #[arg(short, long)]
        voter_index: usize,

        #[arg(short, long)]
        vote_data: u8,
    }
}