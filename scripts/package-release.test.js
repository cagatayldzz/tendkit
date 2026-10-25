import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { artifactNames, finalize, releaseTargets } from "./package-release.js";

test("every desktop target is required before checksums can be published", () => {
  const root = mkdtempSync(join(tmpdir(), "tendkit-assets-"));
  try {
    const names = artifactNames("v1.2.3");
    expect(names).toHaveLength(5);
    expect(releaseTargets.map((target) => `${target.os}-${target.arch}`)).toEqual(["macos-arm64", "macos-x64", "windows-x64", "linux-x64"]);
    for (const name of names.slice(1)) writeFileSync(join(root, name), "artifact\n");
    expect(() => finalize(root, "v1.2.3")).toThrow();
    writeFileSync(join(root, names[0]), "");
    expect(() => finalize(root, "v1.2.3")).toThrow("Empty release asset");
    writeFileSync(join(root, names[0]), "artifact\n");
    expect(finalize(root, "v1.2.3")).toEqual(names);
    const lines = readFileSync(join(root, "SHA256SUMS.txt"), "utf8").trim().split("\n");
    expect(lines).toHaveLength(5);
    expect(lines.every((line) => /^[0-9a-f]{64}  TendKit-/.test(line))).toBe(true);
    writeFileSync(join(root, "unexpected.zip"), "artifact");
    expect(() => finalize(root, "v1.2.3")).toThrow("platform matrix");
    expect(() => artifactNames("v1.2.3/../../file")).toThrow("Invalid release tag");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
