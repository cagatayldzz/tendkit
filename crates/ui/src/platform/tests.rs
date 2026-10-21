#[test]
fn groups_thousands() {
  assert_eq!(super::grouped(4_096), "4,096");
  assert_eq!(super::grouped(128_000), "128,000");
  assert_eq!(super::grouped(512), "512");
}
