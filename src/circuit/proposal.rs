use crate::circuit::tree::*;
use crate::circuit::voter::*;
use crate::circuit::ConstraintF;

use std::borrow::Borrow;

use ark_ff::fields::Fp256;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, Namespace, SynthesisError};
use std::ops::{Mul, Sub};

use ark_crypto_primitives::crh::{CRHGadget, TwoToOneCRH, CRH};

use ark_std::rand::{CryptoRng, RngCore};

#[derive(Clone)]
pub struct Parameters {
    pub leaf_crh_params: <LeafHash as CRH>::Parameters,
    pub two_to_one_crh_params: <TwoToOneHash as TwoToOneCRH>::Parameters,
}
impl Parameters {
    pub fn init<R: CryptoRng + RngCore>(rng: &mut R) -> Parameters {
        let leaf_crh_params = <LeafHash as CRH>::setup(rng).unwrap();
        let two_to_one_crh_params = <TwoToOneHash as TwoToOneCRH>::setup(rng).unwrap();
        Parameters {
            leaf_crh_params,
            two_to_one_crh_params,
        }
    }
}

pub struct ParametersVar {
    pub leaf_crh_params: LeafHashParamsVar,
    pub two_to_one_crh_params: TwoToOneHashParamsVar,
}
impl AllocVar<Parameters, ConstraintF> for ParametersVar {
    #[tracing::instrument(target = "r1cs", skip(cs, f, _mode))]
    fn new_variable<T: Borrow<Parameters>>(
        cs: impl Into<Namespace<ConstraintF>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        _mode: AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let cs = cs.into();
        f().and_then(|params| {
            let params: &Parameters = params.borrow();
            let leaf_crh_params =
                LeafHashParamsVar::new_constant(cs.clone(), &params.leaf_crh_params)?;
            let two_to_one_crh_params =
                TwoToOneHashParamsVar::new_constant(cs.clone(), &params.two_to_one_crh_params)?;
            Ok(Self {
                leaf_crh_params,
                two_to_one_crh_params,
            })
        })
    }
}

#[derive(Clone)]
pub struct ProposalCircuit {
    pub parameters: Parameters,

    // public inputs
    pub root: Option<Root>,
    pub process_id: Option<ProcessId>,
    pub nullifier: Option<Nullifier>,
    pub vote: Option<Vote>,

    // private inputs
    pub sk: Option<SecretKey>,
    pub proof: Option<Proof>,
}

impl ProposalCircuit {
    pub fn public_inputs(self) -> Vec<ConstraintF> {
        vec![
            self.root.unwrap(),
            self.process_id.unwrap(),
            self.nullifier.unwrap(),
            self.vote.unwrap(),
        ]
    }
}

impl ConstraintSynthesizer<ConstraintF> for ProposalCircuit {
    #[tracing::instrument(target = "r1cs", skip(self, cs))]
    fn generate_constraints(
        self,
        cs: ConstraintSystemRef<ConstraintF>,
    ) -> Result<(), SynthesisError> {
        let parameters =
            ParametersVar::new_constant(ark_relations::ns!(cs, "parameters"), &self.parameters)?;

        // public inputs
        let root = RootVar::new_input(ark_relations::ns!(cs, "Root"), || {
            self.root.ok_or(SynthesisError::AssignmentMissing)
        })?;
        let process_id = ProcessIdVar::new_input(ark_relations::ns!(cs, "ProcessId"), || {
            self.process_id.ok_or(SynthesisError::AssignmentMissing)
        })?;
        let nullifier = NullifierVar::new_input(ark_relations::ns!(cs, "Nullifier"), || {
            self.nullifier.ok_or(SynthesisError::AssignmentMissing)
        })?;
        let vote = VoteVar::new_input(ark_relations::ns!(cs, "Vote"), || {
            self.vote.ok_or(SynthesisError::AssignmentMissing)
        })?;

        let proof = ProofVar::new_witness(ark_relations::ns!(cs, "Proof(path)"), || {
            self.proof.ok_or(SynthesisError::AssignmentMissing)
        })?;

        let sk = SecretKeyVar::new_witness(ark_relations::ns!(cs, "SecretKey"), || {
            self.sk.ok_or(SynthesisError::AssignmentMissing)
        })?;

        // check vote value (binary check (v*(v-1)==0))
        let zero: FpVar<ConstraintF> = FpVar::Constant(Fp256::from(0));
        let one: FpVar<ConstraintF> = FpVar::Constant(Fp256::from(1));
        vote.clone().mul(vote.sub(&one)).enforce_equal(&zero)?;

        // check nullifier
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(&sk.to_bytes()?);
        hash_input.extend_from_slice(&process_id.to_bytes()?);
        let comp_nullifier = LeafHashGadget::evaluate(&parameters.leaf_crh_params, &hash_input)?;
        comp_nullifier.enforce_equal(&nullifier)?;

        // obtain voting_key from sk
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(&sk.to_bytes()?);
        let voting_key = LeafHashGadget::evaluate(&parameters.leaf_crh_params, &hash_input)?;

        // verify merkle proof
        proof
            .verify_membership(
                &parameters.leaf_crh_params,
                &parameters.two_to_one_crh_params,
                &root,
                &voting_key.to_bytes().unwrap().as_slice(),
            )?
            .enforce_equal(&Boolean::TRUE)?;

        Ok(())
    }
}