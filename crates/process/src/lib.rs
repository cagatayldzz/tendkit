// Process groups and signals require libc calls.
#![allow(unsafe_code)]

//! Child-process supervision for the engine: cancellable commands that run in
//! their own process groups, network downloads through `curl`, and the search
//! directories used to find developer tools.

mod command;
mod control;
mod environment;
mod network;

pub use command::{command, ProbeGroup};
pub use control::{cancel_on_sigterm, Control};
pub use environment::{home, search_dirs};
pub use network::{curl, get_json};

/// Receives engine events as `(type, data)` pairs.
pub type Sink<'a> = dyn FnMut(&str, serde_json::Value) -> Result<(), String> + 'a;

#[cfg(test)]
#[path = "../tests/commands.rs"]
mod tests;
