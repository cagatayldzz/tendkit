use super::strip_ansi;

#[test]
fn strips_ansi_color_sequences() {
  assert_eq!(strip_ansi("\u{1b}[1;32m==>\u{1b}[0m Pouring"), "==> Pouring");
  assert_eq!(strip_ansi("plain — text"), "plain — text");
}
