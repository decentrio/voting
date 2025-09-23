use anyhow::Result;
use ark_ff::{BigInteger256, BitIteratorBE, Field};
use clap::Subcommand;
use std::path::PathBuf;

use crate::circuit::{ConstraintF, voter::Voter};

#[derive(Debug, Default, Clone)]
pub struct StoredKeypair(pub Vec<u8>);

impl From<BigInteger256> for StoredKeypair {
    fn from(value: BigInteger256) -> Self {
        let base_prime_field = BitIteratorBE::new(ConstraintF::characteristic());
        let mut bits: Vec<bool> = BitIteratorBE::new(value)
            .zip(base_prime_field)
            .skip_while(|(_, c)| !c)
            .map(|(b, _)| b)
            .collect();
        println!("bits: {:?}", bits);
        bits.reverse();

        let field = ConstraintF::characteristic();
        let mut n: usize = 4 * 64;
        let mut bits = vec![];
        let mut started = false;
        while n > 0 {
            n -= 1;
            let part = n / 64;
            let bit = n - (64 * part);
            if !started {
                if (field[part] & (1 << bit) > 0) {
                    started = true;
                    bits.push(value.0[part] & (1 << bit) > 0);
                }
            } else {
                bits.push(value.0[part] & (1 << bit) > 0);
            }
        }

        bits.reverse();
        let out: Vec<u8> = bits
            .chunks(8)
            .map(|chunk| {
                let mut val = 0u8;
                for (i, &bit) in chunk.iter().enumerate() {
                    if bit {
                        val += 1 << i;
                    }
                }
                val
            })
            .collect();
        Self(out)
    }
}

impl StoredKeypair {
    pub fn to_bigint256(self) -> BigInteger256 {
        let mut limbs = [0u64; 4];
        let max_bits = 256;
        for (byte_idx, &byte) in self.0.iter().enumerate() {
            // for each bit inside the byte (LSB-first)
            for bit_in_byte in 0..8 {
                let bit_index = byte_idx * 8 + bit_in_byte;
                if bit_index >= max_bits {
                    break; // ignore anything beyond 256 bits
                }

                if ((byte >> bit_in_byte) & 1) == 1 {
                    let limb_idx = bit_index / 64;
                    let offset = bit_index % 64;
                    limbs[limb_idx] |= 1u64 << offset;
                }
            }
        }
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
    Create,
    Show,
    List,
}

pub struct KeyConfig {
    pub path: PathBuf,
    pub name: Option<String>,
    pub voter: Option<Voter>,
}

impl KeyCommands {
    pub fn handle_command(command: KeyCommands, config: KeyConfig) -> Result<()> {
        let key_storage = RawKeyStorage::new(config.path);

        match command {
            KeyCommands::Create => {
                create(key_storage, config.name.unwrap(), config.voter.unwrap())
            }
            KeyCommands::Show => {
                show(key_storage, config.name.unwrap())
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
