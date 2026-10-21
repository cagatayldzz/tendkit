import { expect, test } from "bun:test";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

test("release preparation selects the tagged range and stamps every package consistently", () => {
  const root = mkdtempSync(join(tmpdir(), "tendkit-release-"));
  const run = (args, env = {}) => Bun.spawnSync(args, {
    cwd: root,
    env: { ...process.env, ...env },
    stdout: "pipe",
    stderr: "pipe"
  });
  const git = (...args) => {
    const result = run(["git", ...args]);
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
    return result.stdout.toString().trim();
  };
  try {
    for (const directory of ["scripts", "apps/tendkit/macos"]) mkdirSync(join(root, directory), { recursive: true });
    for (const file of ["release-ci.js", "release-notes.js"]) {
      copyFileSync(new URL(file, import.meta.url), join(root, "scripts", file));
    }
    writeFileSync(join(root, "package.json"), '{"version":"0.2.0","type":"module"}\n');
    writeFileSync(join(root, "Cargo.toml"), '[workspace.package]\nversion = "0.2.0"\nedition = "2021"\n');
    writeFileSync(join(root, "Cargo.lock"), ["tendkit", "tendkit-protocol", "dependency", "tendkit-engine"]
      .map((name) => `[[package]]\nname = "${name}"\nversion = "${name === "dependency" ? "1.0.0" : "0.2.0"}"\n`).join("\n"));
    writeFileSync(join(root, "apps/tendkit/macos/Info.plist"), '<key>CFBundleShortVersionString</key><string>0.2.0</string>\n<key>CFBundleVersion</key><string>2</string>\n');
    git("init", "--initial-branch=main");
    git("add", ".");
    git("-c", "user.name=Release Test", "-c", "user.email=test@example.com", "commit", "-m", "feat: initial app");
    git("tag", "v0.2.9");
    writeFileSync(join(root, "change.txt"), "Release fixture\n");
    git("add", ".");
    git("-c", "user.name=Release Test", "-c", "user.email=test@example.com", "commit", "-m", "fix: preserve output");
    const sha = git("rev-parse", "HEAD");
    const output = join(root, "github-output");
    const env = { GITHUB_ACTIONS: "true", EXPECTED_SHA: sha, GITHUB_OUTPUT: output, GITHUB_RUN_NUMBER: "42" };
    const result = run([process.execPath, "scripts/release-ci.js", "prepare"], env);
    expect(result.stderr.toString()).toBe("");
    expect(result.exitCode).toBe(0);
    const context = JSON.parse(readFileSync(join(root, "dist/release-context.json"), "utf8"));
    expect(context.tag).toBe("v0.2.10");
    expect(context.previousTag).toBe("v0.2.9");
    expect(context.commits).toEqual([{ sha, subject: "fix: preserve output" }]);
    expect(readFileSync(output, "utf8")).toContain("tag=v0.2.10\n");
    for (const file of ["package.json", "Cargo.toml", "apps/tendkit/macos/Info.plist"]) {
      expect(readFileSync(join(root, file), "utf8")).toContain("0.2.10");
    }
    const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
    for (const name of ["tendkit", "tendkit-protocol", "tendkit-engine"]) {
      expect(lock).toContain(`name = "${name}"\nversion = "0.2.10"`);
    }
    expect(lock).toContain('name = "dependency"\nversion = "1.0.0"');
    expect(readFileSync(join(root, "apps/tendkit/macos/Info.plist"), "utf8")).toContain("<string>42</string>");
    expect(git("rev-parse", "HEAD")).toBe(sha);
    const moved = run([process.execPath, "scripts/release-ci.js", "prepare"], { ...env, EXPECTED_SHA: "0".repeat(40) });
    expect(moved.exitCode).toBe(1);
    expect(moved.stderr.toString()).toContain("does not match the workflow source commit");
    git("tag", "v0.2.10");
    const empty = run([process.execPath, "scripts/release-ci.js", "prepare"], env);
    expect(empty.exitCode).toBe(1);
    expect(empty.stderr.toString()).toContain("no changes");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
