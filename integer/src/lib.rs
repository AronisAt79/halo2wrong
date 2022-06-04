#![feature(trait_alias)]

use crate::rns::{Common, Integer, Limb};
use halo2::{arithmetic::FieldExt, circuit::Cell};
use maingate::{big_to_fe, compose, fe_to_big, Assigned, AssignedValue, UnassignedValue};
use num_bigint::BigUint as big_uint;
use rns::Rns;
use std::rc::Rc;

pub use chip::{IntegerChip, IntegerConfig};
pub use instructions::{IntegerInstructions, Range};
pub use maingate;
pub use maingate::halo2;

pub mod chip;
pub mod instructions;
pub mod rns;

/// `RangeChip` supports upto four full limbs decomposition of a value
/// `AssignedLimb` is mostly subjected to the range check. Say we have 68-bit
/// limb and it is decomposed to four 17-bit limbs.
pub const NUMBER_OF_LOOKUP_LIMBS: usize = 4;

cfg_if::cfg_if! {
  if #[cfg(feature = "kzg")] {
    pub trait WrongExt = halo2::arithmetic::BaseExt;
  } else {
    pub trait WrongExt = halo2::arithmetic::FieldExt;
  }
}

/// AssignedLimb is a limb of an non native integer
#[derive(Debug, Clone)]
pub struct AssignedLimb<F: FieldExt> {
    // Witness value
    value: Option<Limb<F>>,
    // Cell that this value accomadates
    cell: Cell,
    // Maximum value to track overflow and reduction flow
    max_val: big_uint,
}

/// `AssignedLimb` can be also represented as `AssignedValue`
impl<F: FieldExt> From<AssignedLimb<F>> for AssignedValue<F> {
    fn from(limb: AssignedLimb<F>) -> Self {
        AssignedValue::new(limb.cell(), limb.value())
    }
}

/// `AssignedLimb` can be also represented as `AssignedValue`
impl<F: FieldExt> From<&AssignedLimb<F>> for AssignedValue<F> {
    fn from(limb: &AssignedLimb<F>) -> Self {
        AssignedValue::new(limb.cell(), limb.value())
    }
}

impl<F: FieldExt> Assigned<F> for AssignedLimb<F> {
    fn value(&self) -> Option<F> {
        self.value.as_ref().map(|value| value.fe())
    }
    fn cell(&self) -> Cell {
        self.cell
    }
}

impl<F: FieldExt> Assigned<F> for &AssignedLimb<F> {
    fn value(&self) -> Option<F> {
        self.value.as_ref().map(|value| value.fe())
    }
    fn cell(&self) -> Cell {
        self.cell
    }
}

impl<F: FieldExt> AssignedLimb<F> {
    /// Constructs new `AssignedLimb`
    fn new(cell: Cell, value: Option<F>, max_val: big_uint) -> Self {
        let value = value.map(|value| Limb::<F>::new(value));
        AssignedLimb {
            value,
            cell,
            max_val,
        }
    }

    /// Given an assigned value and expected maximum value constructs new
    /// `AssignedLimb`
    fn from(assigned: AssignedValue<F>, max_val: big_uint) -> Self {
        let value = assigned.value().map(|value| Limb::<F>::new(value));
        let cell = assigned.cell();
        AssignedLimb {
            value,
            cell,
            max_val,
        }
    }

    /// Helper functions for maximum value tracking

    fn limb(&self) -> Option<Limb<F>> {
        self.value.clone()
    }

    fn max_val(&self) -> big_uint {
        self.max_val.clone()
    }

    fn add(&self, other: &Self) -> big_uint {
        self.max_val.clone() + other.max_val.clone()
    }

    fn mul2(&self) -> big_uint {
        self.max_val.clone() + self.max_val.clone()
    }

    fn mul3(&self) -> big_uint {
        self.max_val.clone() + self.max_val.clone() + self.max_val.clone()
    }

    fn add_big(&self, other: big_uint) -> big_uint {
        self.max_val.clone() + other
    }

    fn add_fe(&self, other: F) -> big_uint {
        self.add_big(fe_to_big(other))
    }
}

/// Witness integer that is about to be assigned.
#[derive(Debug, Clone)]
pub struct UnassignedInteger<
    W: WrongExt,
    N: FieldExt,
    const NUMBER_OF_LIMBS: usize,
    const BIT_LEN_LIMB: usize,
>(Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>);

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    UnassignedInteger<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    pub fn new(int: Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>) -> Self {
        Self(int)
    }
}

impl<'a, W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    From<Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>>
    for UnassignedInteger<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    fn from(integer: Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>) -> Self {
        UnassignedInteger(integer)
    }
}

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    UnassignedInteger<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    /// Returns value in BigUint form to calculate further witnesses
    fn value(&self) -> Option<big_uint> {
        self.0.as_ref().map(|e| e.value())
    }

    /// Returns indexed limb as ÏUnassignedValue`
    fn limb(&self, idx: usize) -> UnassignedValue<N> {
        self.0.as_ref().map(|e| e.limb(idx).fe()).into()
    }

    /// Returns unassigned value under scalar field modulus
    fn native(&self) -> UnassignedValue<N> {
        self.0.as_ref().map(|integer| integer.native()).into()
    }
}

///
#[derive(Debug, Clone)]
pub struct AssignedInteger<
    W: WrongExt,
    N: FieldExt,
    const NUMBER_OF_LIMBS: usize,
    const BIT_LEN_LIMB: usize,
> {
    // Limbs of the emulated integer
    limbs: [AssignedLimb<N>; NUMBER_OF_LIMBS],
    /// Value in the scalar field
    native_value: AssignedValue<N>,
    /// Share rns across all `AssignedIntegers`s
    rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>,
}

impl<'a, W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    AssignedInteger<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    pub fn new(
        rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>,
        limbs: &[AssignedLimb<N>; NUMBER_OF_LIMBS],
        native_value: AssignedValue<N>,
    ) -> Self {
        AssignedInteger {
            limbs: limbs.clone(),
            native_value,
            rns,
        }
    }

    fn max_val(&self) -> big_uint {
        compose(self.max_vals().to_vec(), BIT_LEN_LIMB)
    }

    fn max_vals(&self) -> [big_uint; NUMBER_OF_LIMBS] {
        self.limbs
            .iter()
            .map(|limb| limb.max_val())
            .collect::<Vec<big_uint>>()
            .try_into()
            .unwrap()
    }

    fn limb(&self, idx: usize) -> AssignedValue<N> {
        self.limbs[idx].clone().into()
    }

    pub fn limbs(&self) -> [AssignedLimb<N>; NUMBER_OF_LIMBS] {
        self.limbs.clone()
    }

    pub fn native(&self) -> AssignedValue<N> {
        self.native_value
    }

    pub fn integer(&self) -> Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>> {
        let has_value = self.limbs[0].value.clone().map(|_| ());
        let limbs: Option<Vec<Limb<N>>> = has_value.map(|_| {
            let limbs = self.limbs.iter().map(|limb| limb.limb().unwrap()).collect();
            limbs
        });
        limbs.map(|limbs| Integer::new(limbs, Rc::clone(&self.rns)))
    }

    pub fn make_aux(&self) -> Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB> {
        let mut max_shift = 0usize;
        let max_vals = self.max_vals();
        for (max_val, aux) in max_vals.iter().zip(self.rns.base_aux.iter()) {
            let mut shift = 1;
            let mut aux = aux.clone();
            while *max_val > aux {
                aux <<= 1usize;
                max_shift = std::cmp::max(shift, max_shift);
                shift += 1;
            }
        }

        Integer::from_limbs(
            &self
                .rns
                .base_aux
                .iter()
                .map(|aux_limb| big_to_fe(aux_limb << max_shift))
                .collect::<Vec<N>>()
                .try_into()
                .unwrap(),
            Rc::clone(&self.rns),
        )
    }
}
