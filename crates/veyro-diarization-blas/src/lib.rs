//! speakrs enables `ndarray/blas` via `ndarray-linalg`; Cargo unifies that onto the
//! shared `ndarray` dependency used by polyvoice, so polyvoice's cdylib needs CBLAS
//! symbols at link time. Referencing `intel-mkl-src` here matches speakrs x86_64 defaults.
#![allow(unused_extern_crates)]
extern crate intel_mkl_src as _mkl_for_ndarray_blas;
