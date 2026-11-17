//! Simulation models for Embedded Lab.
//!
//! Rules for this crate: no GUI dependencies, no threads, no clocks, no file IO.
//! Everything is deterministic (noise comes from a seeded [`rng::Rng`]) so that the
//! same inputs always give the same picture, and so the crate also builds for the web.

pub mod ble;
pub mod bus;
pub mod crypto;
pub mod dsp;
pub mod field;
pub mod firmware;
pub mod gatt;
pub mod mcu;
pub mod mesh;
pub mod modem;
pub mod radar;
pub mod radio;
pub mod rng;
pub mod signals;
pub mod smp;
pub mod units;
pub mod zephyr;
