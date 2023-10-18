#![feature(array_chunks, const_maybe_uninit_write, const_mut_refs, portable_simd)]

/// Chess domain types.
pub mod chess;
/// Neural network for position evaluation.
pub mod nnue;
/// Minimax searching algorithm.
pub mod search;
/// UCI protocol.
pub mod uci;
/// Assorted utilities.
pub mod util;
