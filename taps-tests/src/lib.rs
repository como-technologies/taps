//! taps-level integration tests live in `tests/` — this crate has no
//! library surface. Run them with `just integration` (they are env-gated
//! on `TAPS_INTEGRATION=1` and drive real in-tree binaries over their
//! transports; `cargo test --workspace` skips them).
