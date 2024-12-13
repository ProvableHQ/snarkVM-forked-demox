// Copyright 2024 Aleo Network Foundation
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:

// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::prelude::{Deserialize, DeserializeOwned, Serialize};
use snarkvm_curves::{
    bls12_377::{Bls12_377, Fr},
    edwards_bls12::{EdwardsAffine, EdwardsParameters},
    AffineCurve, MontgomeryParameters, PairingEngine, ProjectiveCurve, TwistedEdwardsParameters,
};
use snarkvm_fields::{field, PrimeField, SquareRootField};
use snarkvm_utilities::{BigInteger, BigInteger256};

use core::{fmt::Debug, hash::Hash};
use zeroize::Zeroize;

pub trait Environment:
    'static + Copy + Clone + Debug + PartialEq + Eq + Hash + Serialize + DeserializeOwned + Send + Sync
{
    type Affine: AffineCurve<
        Projective = Self::Projective,
        BaseField = Self::Field,
        ScalarField = Self::Scalar,
        Coordinates = (Self::Field, Self::Field),
    >;
    type BigInteger: BigInteger;
    type Field: PrimeField<BigInteger = Self::BigInteger> + SquareRootField + Copy + Zeroize;
    type PairingCurve: PairingEngine<Fr = Self::Field>;
    type Projective: ProjectiveCurve<Affine = Self::Affine, BaseField = Self::Field, ScalarField = Self::Scalar>;
    type Scalar: PrimeField<BigInteger = Self::BigInteger> + Copy + Zeroize;

    /// The coefficient `A` of the twisted Edwards curve.
    const EDWARDS_A: Self::Field;
    /// The coefficient `D` of the twisted Edwards curve.
    const EDWARDS_D: Self::Field;

    /// The coefficient `A` of the Montgomery curve.
    const MONTGOMERY_A: Self::Field;
    /// The coefficient `B` of the Montgomery curve.
    const MONTGOMERY_B: Self::Field;

    /// The maximum number of bytes allowed in a string.
    const MAX_STRING_BYTES: u32 = u8::MAX as u32;

    const PRECOMPUTED_FIRST_POSEIDON_ROUND: [Self::Field; 9];

    /// Halts the program from further synthesis, evaluation, and execution in the current environment.
    fn halt<S: Into<String>, T>(message: S) -> T {
        panic!("{}", message.into())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Console;

impl Environment for Console {
    type Affine = EdwardsAffine;
    type BigInteger = <Self::Field as PrimeField>::BigInteger;
    type Field = <Self::Affine as AffineCurve>::BaseField;
    type PairingCurve = Bls12_377;
    type Projective = <Self::Affine as AffineCurve>::Projective;
    type Scalar = <Self::Affine as AffineCurve>::ScalarField;

    /// The coefficient `A` of the twisted Edwards curve.
    const EDWARDS_A: Self::Field = <EdwardsParameters as TwistedEdwardsParameters>::EDWARDS_A;
    /// The coefficient `D` of the twisted Edwards curve.
    const EDWARDS_D: Self::Field = <EdwardsParameters as TwistedEdwardsParameters>::EDWARDS_D;
    /// The coefficient `A` of the Montgomery curve.
    const MONTGOMERY_A: Self::Field = <EdwardsParameters as MontgomeryParameters>::MONTGOMERY_A;
    /// The coefficient `B` of the Montgomery curve.
    const MONTGOMERY_B: Self::Field = <EdwardsParameters as MontgomeryParameters>::MONTGOMERY_B;
    /// The first round of poseidon computations is constant, as the first input is always the encryption domain, which is constant
    const PRECOMPUTED_FIRST_POSEIDON_ROUND: [Self::Field; 9] = [
        field!(Fr, BigInteger256([18400883315055711526u64, 2190994228000969506u64, 5206877399204740241u64, 639223570235065050u64])),
        field!(Fr, BigInteger256([15399590913876696331u64, 15594399395362219105u64, 9529204955545525528u64, 669261489487838935u64])),
        field!(Fr, BigInteger256([6602557572627535518u64, 9929926631054717162u64, 4515907751056267905u64, 663106168251105715u64])),
        field!(Fr, BigInteger256([6993747136410928348u64, 18065552776694758143u64, 2408644198368128959u64, 632536464969372654u64])),
        field!(Fr, BigInteger256([7545327712549990254u64, 4761540785546221362u64, 8841461201722074u64, 1306050386496310916u64])),
        field!(Fr, BigInteger256([18213014258197997104u64, 13996994921836506268u64, 17734367086959815498u64, 683479563726195903u64])),
        field!(Fr, BigInteger256([8891171731444413293u64, 15005596550599256007u64, 8255404474136563175u64, 115883256545935591u64])),
        field!(Fr, BigInteger256([4511196639787990753u64, 13695034759934762127u64, 17467001190581089751u64, 955255359426287200u64])),
        field!(Fr, BigInteger256([9546547704158379798u64, 16739824303501531796u64, 16409388477121326005u64, 440214594306550130u64])),
    ];
}
