use crate::chess::Move;
use crate::util::{Binary, Bits};
use derive_more::{Display, Error};
use std::{cmp::Ordering, ops::RangeInclusive};
use test_strategy::Arbitrary;

mod iter;
mod table;

pub use iter::*;
pub use table::*;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Arbitrary)]
enum Kind {
    Lower,
    Upper,
    Exact,
}

/// A partial search result.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Arbitrary)]
pub struct Transposition {
    kind: Kind,
    #[strategy(0..=Self::MAX_DEPTH)]
    depth: u8,
    score: i16,
    best: Move,
}

impl PartialOrd for Transposition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (self.depth, self.kind).partial_cmp(&(other.depth, other.kind))
    }
}

impl Ord for Transposition {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.depth, self.kind).cmp(&(other.depth, other.kind))
    }
}

impl Transposition {
    pub const MAX_DEPTH: u8 = (u8::MAX >> 3);

    fn new(kind: Kind, score: i16, depth: u8, best: Move) -> Self {
        assert!(depth <= Self::MAX_DEPTH, "{} <= {}", depth, Self::MAX_DEPTH);

        Transposition {
            kind,
            depth,
            score,
            best,
        }
    }

    /// Constructs a [`Transposition`] given a lower bound for the score, depth searched, and best [`Move`].
    pub fn lower(score: i16, depth: u8, best: Move) -> Self {
        Transposition::new(Kind::Lower, score, depth, best)
    }

    /// Constructs a [`Transposition`] given an upper bound for the score, depth searched, and best [`Move`].
    pub fn upper(score: i16, depth: u8, best: Move) -> Self {
        Transposition::new(Kind::Upper, score, depth, best)
    }

    /// Constructs a [`Transposition`] given the exact score, depth searched, and best [`Move`].
    pub fn exact(score: i16, depth: u8, best: Move) -> Self {
        Transposition::new(Kind::Exact, score, depth, best)
    }

    /// Bounds for the exact score.
    pub fn bounds(&self) -> RangeInclusive<i16> {
        match self.kind {
            Kind::Lower => self.score..=i16::MAX,
            Kind::Upper => i16::MIN..=self.score,
            Kind::Exact => self.score..=self.score,
        }
    }

    /// Depth searched.
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// Partial score.
    pub fn score(&self) -> i16 {
        self.score
    }

    /// Best [`Move`] at this depth.
    pub fn best(&self) -> Move {
        self.best
    }
}

type Signature = Bits<26>;
type OptionalSignedTransposition = Option<(Transposition, Signature)>;

/// The reason why decoding [`Transposition`] from binary failed.
#[derive(Debug, Display, Clone, Eq, PartialEq, Arbitrary, Error)]
#[display(fmt = "not a valid transposition")]
pub struct DecodeTranspositionError;

impl From<<Move as Binary>::Error> for DecodeTranspositionError {
    fn from(_: <Move as Binary>::Error) -> Self {
        DecodeTranspositionError
    }
}

impl Binary for OptionalSignedTransposition {
    type Bits = Bits<64>;
    type Error = DecodeTranspositionError;

    fn encode(&self) -> Self::Bits {
        match self {
            None => Bits::default(),
            Some((t, sig)) => {
                let mut bits = Bits::default();
                bits.push(*sig);
                bits.push(t.best.encode());
                bits.push(Bits::<16>::new(t.score as u16 as _));
                bits.push(Bits::<5>::new(t.depth as _));
                bits.push(Bits::<2>::new(t.kind as _));

                debug_assert_ne!(bits, Bits::default());

                bits
            }
        }
    }

    fn decode(mut bits: Self::Bits) -> Result<Self, Self::Error> {
        if bits == Bits::default() {
            Ok(None)
        } else {
            Ok(Some((
                Transposition {
                    kind: [Kind::Lower, Kind::Upper, Kind::Exact]
                        .into_iter()
                        .nth(bits.pop::<2>().into())
                        .ok_or(DecodeTranspositionError)?,
                    depth: bits.pop::<5>().into(),
                    score: u16::from(bits.pop::<16>()) as _,
                    best: Move::decode(bits.pop())?,
                },
                bits.pop(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_strategy::proptest;

    #[proptest]
    fn lower_constructs_lower_bound_transposition(
        s: i16,
        #[strategy(0..=Transposition::MAX_DEPTH)] d: u8,
        m: Move,
    ) {
        assert_eq!(
            Transposition::lower(s, d, m),
            Transposition::new(Kind::Lower, s, d, m)
        );
    }

    #[proptest]
    fn upper_constructs_upper_bound_transposition(
        s: i16,
        #[strategy(0..=Transposition::MAX_DEPTH)] d: u8,
        m: Move,
    ) {
        assert_eq!(
            Transposition::upper(s, d, m),
            Transposition::new(Kind::Upper, s, d, m)
        );
    }

    #[proptest]
    fn exact_constructs_exact_transposition(
        s: i16,
        #[strategy(0..=Transposition::MAX_DEPTH)] d: u8,
        m: Move,
    ) {
        assert_eq!(
            Transposition::exact(s, d, m),
            Transposition::new(Kind::Exact, s, d, m)
        );
    }

    #[proptest]
    #[should_panic]
    fn transposition_panics_if_depth_grater_than_max(
        k: Kind,
        s: i16,
        #[strategy(Transposition::MAX_DEPTH + 1..)] d: u8,
        m: Move,
    ) {
        Transposition::new(k, s, d, m);
    }

    #[proptest]
    fn transposition_score_is_between_bounds(t: Transposition) {
        assert!(t.bounds().contains(&t.score()));
    }

    #[proptest]
    fn transposition_with_larger_depth_is_larger(
        t: Transposition,
        #[filter(#t.depth != #u.depth)] u: Transposition,
    ) {
        assert_eq!(t < u, t.depth < u.depth);
    }

    #[proptest]
    fn transposition_with_same_depth_is_compared_by_kind(
        t: Transposition,
        #[filter(#t.depth == #u.depth)] u: Transposition,
    ) {
        assert_eq!(t < u, t.kind < u.kind);
    }

    #[proptest]
    fn decoding_encoded_transposition_is_an_identity(t: OptionalSignedTransposition) {
        assert_eq!(Binary::decode(t.encode()), Ok(t));
    }
}
