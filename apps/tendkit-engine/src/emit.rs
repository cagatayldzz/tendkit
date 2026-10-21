use serde_json::json;
use std::io::{self, Write};
use tendkit_protocol::PROTOCOL_VERSION;

/// Writes one JSON Lines event to stdout, which is reserved for protocol messages.
pub fn emit(request_id: &str, sequence: &mut u32, kind: &str, data: serde_json::Value) -> io::Result<()> {
  *sequence += 1;
  let mut out = io::stdout().lock();
  serde_json::to_writer(
    &mut out,
    &json!({
      "protocol_version": PROTOCOL_VERSION, "request_id": request_id,
      "sequence": sequence, "type": kind, "data": data
    }),
  )?;
  writeln!(out)?;
  out.flush()
}
