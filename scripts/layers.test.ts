import { expect, test } from "bun:test";
import { policyFromMetadata, validateLayers, type Metadata, type Policy } from "./layers";

function fixture(): { metadata: Metadata; rules: Policy } {
  return {
    metadata: {
      workspace_members: ["protocol", "client"],
      packages: [
        { id: "protocol", name: "protocol", manifest_path: "/repo/crates/protocol/Cargo.toml", dependencies: [] },
        { id: "client", name: "client", manifest_path: "/repo/crates/client/Cargo.toml", dependencies: [
          { name: "protocol", kind: null, source: null, path: "/repo/crates/protocol" },
          { name: "tempfile", kind: "dev", source: "registry" },
        ] },
      ],
    },
    rules: { protocol: { dependencies: [] }, client: { dependencies: ["protocol"], devDependencies: ["tempfile"] } },
  };
}

test("a lower layer and a test-only dependency are accepted", () => {
  const { metadata, rules } = fixture();
  expect(validateLayers(metadata, rules)).toEqual([]);
});

test("an unlisted dependency and a test tool moved into production are rejected", () => {
  const { metadata, rules } = fixture();
  metadata.packages[0].dependencies.push({ name: "gpui-kit", kind: null, source: "registry", target: "cfg(windows)" });
  metadata.packages[1].dependencies[1].kind = null;
  const errors = validateLayers(metadata, rules);
  expect(errors.some(error => error.includes("protocol → gpui-kit"))).toBe(true);
  expect(errors.some(error => error.includes("client → tempfile"))).toBe(true);
});

test("a build dependency cannot use the normal dependency allowance", () => {
  const { metadata, rules } = fixture();
  metadata.packages[1].dependencies[0].kind = "build";
  expect(validateLayers(metadata, rules).some(error => error.includes("build dependency is not allowed"))).toBe(true);
});

test("a new crate needs an explicit layer decision", () => {
  const { metadata, rules } = fixture();
  metadata.workspace_members.push("extra");
  metadata.packages.push({ id: "extra", name: "extra", manifest_path: "/repo/crates/extra/Cargo.toml", dependencies: [] });
  expect(validateLayers(metadata, rules)).toContain("extra: has no layer rule.");
});

test("cycles and local packages outside the workspace are rejected even when allowed", () => {
  const { metadata, rules } = fixture();
  rules.protocol.dependencies.push("client");
  metadata.packages[0].dependencies.push({ name: "client", kind: null, source: null, path: "/repo/crates/client" });
  expect(validateLayers(metadata, rules).some(error => error.includes("cycle"))).toBe(true);
  metadata.packages[0].dependencies[0].path = "/elsewhere/client";
  expect(validateLayers(metadata, rules).some(error => error.includes("must be a workspace member"))).toBe(true);
});

test("the allow-list comes from the Cargo.toml metadata table; a package without one has no rule", () => {
  const { metadata } = fixture();
  metadata.packages[0].metadata = { layers: { dependencies: [] } };
  metadata.packages[1].metadata = { layers: { dependencies: ["protocol"], "dev-dependencies": ["tempfile"] } };
  const rules = policyFromMetadata(metadata);
  expect(rules.client).toEqual({ dependencies: ["protocol"], devDependencies: ["tempfile"], buildDependencies: undefined });
  expect(validateLayers(metadata, rules)).toEqual([]);
  metadata.packages[1].metadata = null;
  expect(validateLayers(metadata, policyFromMetadata(metadata))).toContain("client: has no layer rule.");
});
