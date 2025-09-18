use clap::{Subcommand};

use crate::{cmd::key::{KeyCommands}};

pub mod key;


#[derive(Subcommand)]
pub(crate) enum Commands {
    Key {
        #[command(subcommand)]
        command: KeyCommands,
    },

}
