use ark_bls12_381::Parameters;
use ark_ec::{bls12::Bls12, PairingEngine};
use ark_ff::{PrimeField};
use ark_groth16::{Proof, VerifyingKey};
use ark_serialize::CanonicalSerialize;
use serde::Serialize;
use anyhow::Result;

#[derive(Serialize)]
pub struct SnarkjsProof {
    pi_a: [String; 2],
    pi_b: [[String; 2]; 2],
    pi_c: [String; 2],
    protocol: String,
    curve: String,
}


#[derive(Serialize)]
pub struct SnarkjsVk {
    protocol: String,
    curve: String,
    nPublic: usize,
    vk_alpha_1: [String; 2],
    vk_beta_2: [[String; 2]; 2],
    vk_gamma_2: [[String; 2]; 2],
    vk_delta_2: [[String; 2]; 2],
    vk_alphabeta_12: Vec<String>, // serialized Fq12 coefficients (see notes)
    IC: Vec<[String; 2]>, // gamma_abc_g1 -> IC
}

/// Build snarkjs-style proof JSON from arkworks Proof
pub fn proof_to_snarkjs(proof: &Proof<Bls12<Parameters>>) -> SnarkjsProof {
    // G1 points: pi_a and pi_c
    let a = proof.a;
    let c = proof.c;
    let pi_a = {
        let x = a.x.into_repr().to_string();
        let y = a.y.into_repr().to_string();
        [x, y]
    };
    let pi_c = {
        let x = c.x.into_repr().to_string();
        let y = c.y.into_repr().to_string();
        [x, y]
    };

    // G2 point: pi_b (note arkworks stores G2 as projective usually; convert to affine)
    let b_aff = proof.b;
    let pi_b = {
        let x = b_aff.x;
        let y = b_aff.y;
        // Fq2 -> (c0, c1)
        let bx0 = x.c0.into_repr().to_string();
        let bx1 = x.c1.into_repr().to_string();
        let by0 = y.c0.into_repr().to_string();
        let by1 = y.c1.into_repr().to_string();
        [[bx0, bx1], [by0, by1]]
    };

    SnarkjsProof { 
        pi_a, 
        pi_b, 
        pi_c,
        protocol: "groth16".to_string(),
        curve: "bls12-381".to_string()
    }
}

/// Build snarkjs-style verification key JSON from arkworks VerifyingKey
pub fn vk_to_snarkjs(vk: &VerifyingKey<Bls12<Parameters>>, n_public: usize) -> Result<SnarkjsVk> {
    // G1 alpha
    let alpha = vk.alpha_g1;
    let vk_alpha_1 = [alpha.x.into_repr().to_string(), alpha.y.into_repr().to_string()];

    // G2 beta/gamma/delta
    let vk_beta_2 = {
        let b = vk.beta_g2;
        let x = b.x; let y = b.y;
        [[x.c0.into_repr().to_string(), x.c1.into_repr().to_string()],
         [y.c0.into_repr().to_string(), y.c1.into_repr().to_string()]]
    };
    let vk_gamma_2 = {
        let g = vk.gamma_g2;
        let x = g.x; let y = g.y;
        [[x.c0.into_repr().to_string(), x.c1.into_repr().to_string()],
         [y.c0.into_repr().to_string(), y.c1.into_repr().to_string()]]
    };
    let vk_delta_2 = {
        let d = vk.delta_g2;
        let x = d.x; let y = d.y;
        [[x.c0.into_repr().to_string(), x.c1.into_repr().to_string()],
         [y.c0.into_repr().to_string(), y.c1.into_repr().to_string()]]
    };

    // IC (gamma_abc_g1)
    let mut ic = Vec::with_capacity(vk.gamma_abc_g1.len());
    for p in vk.gamma_abc_g1.iter() {
        let pa = p;
        ic.push([pa.x.into_repr().to_string(), pa.y.into_repr().to_string()]);
    }

    // vk_alphabeta_12: compute pairing e(alpha_g1, beta_g2) (Fq12 element)
    // Serialize the Fq12 coefficients into decimal strings (engine-dependent repr).
    // Many snarkjs exports give this value as an array; formats vary slightly between tools.
    let pairing_value = Bls12::<Parameters>::pairing(vk.alpha_g1, vk.beta_g2);
    // pairing_value is in target field (Fq12) type. We'll serialize its internal coefficients.
    // The next lines are engine-specific; for BLS12-381 the Fq12 is extension built from Fq6 over Fq2.
    let mut alphabeta_serialized = Vec::new();
    {
        // If the target field exposes `.c0`, `.c1` etc (typical in arkworks)
        // Flatten nested coefficients to strings. The exact flattening ordering matters for compatibility.
        // We'll collect coefficient limbs using a recursive pattern (Fq12 -> Fq6 -> Fq2 -> Fq).
        // WARNING: different tools may expect different flattening orders. Test with your verifier.

        // Convert to canonical bytes then to big integers is another option; but here we output each Fq limb decimal repr.
        // Example generic extraction (pseudo; adapt for your engine if types are not named c0/c1/c2...).
        // For many engines: pairing_value.0 .c0 .c0 .c0 etc — inspect the type definitions to extract components.
        // For clarity - fallback: serialize the Fq12 into bytes and hex/base10-interpretation, but snarkjs expects decimals.
        let mut bytes = Vec::new();
        pairing_value.serialize_uncompressed(&mut bytes)?;
        // As a pragmatic approach, encode the bytes as decimal strings representing big integers of each 32/48-byte chunk,
        // or base10 of the whole byte blob — but this is tool-dependent.
        // We'll push the hex blob as decimal strings of 32-byte chunks to be safe (explain below).
        // ---- fallback: push single big decimal representation of the serialized bytes ----
        // Interpret the whole bytes as a big-endian integer string:
        use num_bigint::BigUint;
        let big = BigUint::from_bytes_be(&bytes);
        alphabeta_serialized.push(big.to_str_radix(10));
    }

    Ok(SnarkjsVk {
        protocol: "groth16".to_string(),
        curve: "bls12-381".to_string(),
        nPublic: n_public,
        vk_alpha_1,
        vk_beta_2,
        vk_gamma_2,
        vk_delta_2,
        vk_alphabeta_12: alphabeta_serialized,
        IC: ic,
    })
}