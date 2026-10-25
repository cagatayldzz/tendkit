// Signalling the engine process requires a libc call.
#![allow(unsafe_code)]

//! The application's side of the engine protocol: starting the engine process,
//! sending requests and control messages, and validating the event stream.

mod connection;
#[cfg(any(embedded_engine, test))]
mod embedded;
mod validator;

pub use connection::{engine_executable, EngineConnection, Request};
pub use validator::EventValidator;
