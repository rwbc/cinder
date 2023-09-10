use crate::nnue::Layer;
use num_traits::PrimInt;

/// Damps neuron activation.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(test, derive(test_strategy::Arbitrary))]
pub struct Damp<L, const SCALE: i8>(pub(super) L);

impl<L, I, T, const N: usize, const SCALE: i8> Layer<I> for Damp<L, SCALE>
where
    L: Layer<I, Output = [T; N]>,
    T: PrimInt + From<i8>,
{
    type Output = [T; N];

    fn forward(&self, input: I) -> Self::Output {
        self.0.forward(input).map(|v| v / SCALE.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nnue::Passthrough;
    use test_strategy::proptest;

    #[proptest]
    fn damp_scales(l: Damp<Passthrough, 8>, i: [i8; 3]) {
        assert_eq!(l.forward(i), i.map(|v| v / 8));
    }
}
