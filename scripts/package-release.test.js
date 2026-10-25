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

test("Windows packaging passes paths as arguments without requiring an environment variable", async () => {
  const { packageWindows } = await import("./package-release.js");
  const root = mkdtempSync(join(tmpdir(), "tendkit windows' package-"));
  const archive = join(root, "TendKit release.zip");
  const source = join(root, "TendKit");
  try {
    packageWindows(source, archive, (args) => {
      expect(args[0]).toBe("powershell.exe");
      expect(args).toContain("-File");
      expect(args).not.toContain("-Command");
      expect(args[args.indexOf("-SourcePath") + 1]).toBe(source);
      expect(args[args.indexOf("-ArchivePath") + 1]).toBe(archive);
      expect(args.some((arg) => arg.includes("$env:"))).toBe(false);
      // A child process sees complete paths even with spaces and apostrophes.
      const child = Bun.spawnSync([process.execPath, "-e", "console.log(JSON.stringify(process.argv.slice(1)))", ...args], {
        stdout: "pipe",
        stderr: "pipe"
      });
      expect(child.exitCode).toBe(0);
      expect(JSON.parse(child.stdout.toString())).toEqual(args);
      writeFileSync(archive, "archive fixture");
    });
    expect(() => packageWindows(source, join(root, "missing.zip"), () => {})).toThrow("did not produce an archive");
    expect(() => packageWindows(source, "", () => {})).toThrow("requires source and archive paths");
    expect(() => packageWindows(source, archive, () => { throw new Error("Compression failed"); })).toThrow("Compression failed");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
