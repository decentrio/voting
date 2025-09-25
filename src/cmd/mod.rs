use clap::{Subcommand};

use crate::{cmd::{
    key::{KeyCommands},
    vote::{VoteCommands}}
};

pub mod key;
pub mod vote;

#[derive(Subcommand)]
pub(crate) enum Commands {
    Key {
        #[command(subcommand)]
        command: KeyCommands,
    },
    Vote {
        #[command(subcommand)]
        command: VoteCommands,
    },
    Spam
}
