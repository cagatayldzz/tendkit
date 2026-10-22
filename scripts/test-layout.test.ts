import { expect, test } from "bun:test";
import { validateTestLayout, validateRustSpacing } from "./test-layout";

const unitFiles = {
  "src/lib.rs": '#[cfg(test)]\n#[path = "../tests/detection.rs"]\nmod tests;\n',
  "tests/detection.rs": "#[test]\nfn detects_tools() {}\n",
};

test("external unit modules and explicit integration tests can coexist", () => {
  expect(validateTestLayout({ ...unitFiles, "tests/protocol.rs": "#[test]\nfn reads_events() {}" }, {
    package: { autotests: false }, test: [{ name: "protocol" }],
  })).toEqual([]);
});

test("normal Cargo integration test discovery remains available", () => {
  expect(validateTestLayout({ "src/lib.rs": "", "tests/protocol.rs": "#[test]\nfn reads_events() {}" }, {})).toEqual([]);
});

test("source test files and inline tests are rejected", () => {
  for (const files of [
    { "src/tests.rs": "" },
    { "src/platform/tests.rs": "" },
    { "src/tests/helpers.rs": "" },
    { "src/lib.rs": "#[test]\nfn detects_tools() {}" },
    { "src/lib.rs": "#[cfg(test)]\nmod tests {}" },
    { "src/lib.rs": "#[tokio::test]\nasync fn detects_tools() {}" },
  ]) {
    expect(validateTestLayout(files, {}).some(error => error.includes("move test implementations"))).toBe(true);
  }
});

test("unit modules cannot accidentally become integration targets", () => {
  expect(validateTestLayout(unitFiles, {}).some(error => error.includes("must not also be an integration"))).toBe(true);
  expect(validateTestLayout(unitFiles, {
    package: { autotests: false }, test: [{ name: "detection" }],
  }).some(error => error.includes("must not also be an integration"))).toBe(true);
});

test("unregistered tests cannot silently disappear from Cargo test runs", () => {
  expect(validateTestLayout({ "tests/detection.rs": unitFiles["tests/detection.rs"] }, {
    package: { autotests: false },
  }).some(error => error.includes("not registered"))).toBe(true);
});

test("missing modules and misplaced explicit test targets are rejected", () => {
  expect(validateTestLayout({ "src/lib.rs": unitFiles["src/lib.rs"] }, {
    package: { autotests: false },
  }).some(error => error.includes("does not exist"))).toBe(true);
  expect(validateTestLayout({ "src/protocol.rs": "" }, {
    test: [{ name: "protocol", path: "src/protocol.rs" }],
  }).some(error => error.includes("must live inside tests/"))).toBe(true);
});

test("test file names describe the subject", () => {
  expect(validateTestLayout({ "tests/tests.rs": "" }, {}).some(error => error.includes("descriptive test file name"))).toBe(true);
});

test("test items have blank lines without separating attributes from their items", () => {
  expect(validateRustSpacing("use super::*;\n#[test]\nfn first() {\n}\n#[test]\nfn second() {\n}\n")).toEqual([2, 5]);
  expect(validateRustSpacing("use super::*;\n\n#[test]\nfn first() {\n}\n\n#[test]\nfn second() {\n}\n")).toEqual([]);
  expect(validateRustSpacing("use super::*;\nuse std::fs;\n\n#[derive(Default)]\nstruct Fake {\n}\nimpl Fake {\n}\nfn helper() {\n}\n")).toEqual([7, 9]);
});

test("Rust methods and trait signatures are separated at the same indentation", () => {
  expect(validateRustSpacing("impl Fake {\n  fn first() {\n  }\n  fn second() {\n  }\n}\n")).toEqual([4]);
  expect(validateRustSpacing("trait Fake {\n  fn first();\n  fn second();\n}\n")).toEqual([3]);
  expect(validateRustSpacing("fn outer() {\n  if true {\n  }\n}\n")).toEqual([]);
});
