
use std::path::PathBuf;
use anyhow::Result;
use ark_ff::{BigInteger256, BitIteratorBE, Field};
use clap::Subcommand;

use crate::circuit::{voter::Voter, ConstraintF};

#[derive(Debug, Clone)]
pub struct StoredKeypair (Vec<u8>);

impl From<BigInteger256> for StoredKeypair {
    fn from(value: BigInteger256) -> Self {
        let base_prime_field = BitIteratorBE::new(ConstraintF::characteristic());
        let mut bits: Vec<bool> = BitIteratorBE::new(value)
            .zip(base_prime_field)
            .skip_while(| (_, c) | !c)
            .map(|(b, _)| b)
            .collect();
        bits.reverse();
        let out: Vec<u8> = bits
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
        Self(out)
    }
}

impl StoredKeypair {
    fn to_bigint256(self) -> BigInteger256 {
        let mut limbs = [0u64; 4];
        // TODO: handle bigint256
        BigInteger256::new(limbs)
    }
}
pub struct RawKeyStorage {
    path: PathBuf,
}

impl RawKeyStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

pub trait KeyStorage {
    fn save_keypair(&self, name: &str, keypair: &StoredKeypair) -> Result<()>;
    fn load_keypair(&self, name: &str) -> Result<StoredKeypair>;
    fn list_keys(&self) -> Result<Vec<String>>;
}

impl KeyStorage for RawKeyStorage {
    fn save_keypair(&self, name: &str, keypair: &StoredKeypair) -> Result<()> {
        let mut path = self.path.clone();
        path.push(name);
        std::fs::write(path, &keypair.0)?;
        Ok(())
    }

    fn load_keypair(&self, name: &str) -> Result<StoredKeypair> {
        let mut path = self.path.clone();
        path.push(name);
        let data = std::fs::read(path)?;
        Ok(StoredKeypair(data))
    }

    fn list_keys(&self) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    keys.push(name.to_string());
                }
            }
        }
        Ok(keys)
    }
    
}

#[derive(Clone, Subcommand)]
pub enum KeyCommands {
    Create {
        #[arg(short, long)]
        name: Option<String>,
    },
    Show {
        #[arg(short, long)]
        name: Option<String>,
    },
    List,
}

pub struct KeyConfig {
    path: PathBuf,
    name: String,
    voter: Voter
}

impl KeyCommands {
    pub fn handle_command(command: KeyCommands, config: KeyConfig) -> Result<()> {
        let key_storage = RawKeyStorage::new(config.path);

        match command {
            KeyCommands::Create { name } => {
                let key_name = name.unwrap_or_else(|| config.name);

                create(key_storage, key_name, config.voter)
            }
            KeyCommands::Show { name } => {
                let key_name = name.unwrap_or_else(|| config.name);
                show(key_storage, key_name)
            }
            KeyCommands::List => list(key_storage),
        }
    }
}


fn list<T: KeyStorage>(storage: T) -> Result<()> {
    let keys = storage.list_keys()?;
    for key in keys {
        println!("{}", key);
    }
    Ok(())
}

fn create<T: KeyStorage>(storage: T, name: String, voter: Voter) -> Result<()> {
    let keypair = StoredKeypair::from(voter.sk.0);
    storage.save_keypair(&name, &keypair)?;
    println!("Key {} created.", name,);
    Ok(())
}

fn show<T: KeyStorage>(storage: T, name: String) -> Result<()> {
    let keypair = storage.load_keypair(&name)?;
    println!("Loaded key {}: {:?}", name, keypair);
    Ok(())
}
