use ark_ec::pairing::Pairing;
use std::{fs::File, path::Path};

use super::{CircomCircuit, R1CS};

use num_bigint::BigInt;
use std::collections::HashMap;

use crate::{circom::R1CSFile, witness::WitnessCalculator};
use color_eyre::Result;

#[derive(Clone, Debug)]
pub enum Inputs {
    BigInt(BigInt),
    BigIntVec(Vec<BigInt>),
    BigIntVecVec(Vec<Vec<BigInt>>),
}

impl Inputs {
    pub fn flatten(&self) -> Vec<BigInt> {
        match self {
            Inputs::BigInt(n) => vec![n.clone()],
            Inputs::BigIntVec(n) => n.clone(),
            Inputs::BigIntVecVec(n) => n.clone().into_iter().flatten().collect(),
        }
    }
}

impl std::iter::FromIterator<BigInt> for Inputs {
    fn from_iter<I: IntoIterator<Item = BigInt>>(iter: I) -> Self {
        let items: Vec<BigInt> = iter.into_iter().collect();

        match items.len() {
            0 => panic!("Cannot create Value from an empty iterator"),
            1 => Inputs::BigInt(items[0].clone()),
            2 => Inputs::BigIntVec(items),
            _ => panic!("Cannot create Value from an iterator with more than 3 elements"),
        }
    }
}

#[derive(Debug)]
pub struct CircomBuilder<E: Pairing> {
    pub cfg: CircomConfig<E>,
    pub inputs: HashMap<String, Inputs>,
}

// Add utils for creating this from files / directly from bytes
#[derive(Debug)]
pub struct CircomConfig<E: Pairing> {
    pub r1cs: R1CS<E>,
    pub wtns: WitnessCalculator,
    pub sanity_check: bool,
}

impl<E: Pairing> CircomConfig<E> {
    pub fn new(wtns: impl AsRef<Path>, r1cs: impl AsRef<Path>) -> Result<Self> {
        let wtns = WitnessCalculator::new(wtns).unwrap();
        let reader = File::open(r1cs)?;
        let r1cs = R1CSFile::new(reader)?.into();
        Ok(Self {
            wtns,
            r1cs,
            sanity_check: false,
        })
    }
}

impl<E: Pairing> CircomBuilder<E> {
    /// Instantiates a new builder using the provided WitnessGenerator and R1CS files
    /// for your circuit
    pub fn new(cfg: CircomConfig<E>) -> Self {
        Self {
            cfg,
            inputs: HashMap::new(),
        }
    }

    /// Pushes a Circom input at the specified name.
    pub fn push_input(&mut self, name: impl ToString, val: Inputs) {
        self.inputs.entry(name.to_string()).or_insert(val);
    }

    /// Generates an empty circom circuit with no witness set, to be used for
    /// generation of the trusted setup parameters
    pub fn setup(&self) -> CircomCircuit<E> {
        let mut circom = CircomCircuit {
            r1cs: self.cfg.r1cs.clone(),
            witness: None,
        };

        // Disable the wire mapping
        circom.r1cs.wire_mapping = None;

        circom
    }

    /// Creates the circuit populated with the witness corresponding to the previously
    /// provided inputs
    pub fn build(mut self) -> Result<CircomCircuit<E>> {
        let mut circom = self.setup();

        // calculate the witness
        let witness = self
            .cfg
            .wtns
            .calculate_witness_element::<E, _>(self.inputs, self.cfg.sanity_check)?;
        circom.witness = Some(witness);

        // sanity check
        debug_assert!({
            use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem};
            let cs = ConstraintSystem::<E::ScalarField>::new_ref();
            circom.clone().generate_constraints(cs.clone()).unwrap();
            let is_satisfied = cs.is_satisfied().unwrap();
            if !is_satisfied {
                println!(
                    "Unsatisfied constraint: {:?}",
                    cs.which_is_unsatisfied().unwrap()
                );
            }

            is_satisfied
        });

        Ok(circom)
    }
}
