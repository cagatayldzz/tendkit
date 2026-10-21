import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { generateNotes, maxCommits, nextVersion, releaseBump, renderNotes } from "./release-notes.js";

process.chdir(fileURLToPath(new URL("..", import.meta.url)));

function git(...args) {
  const result = Bun.spawnSync(["git", ...args], { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(`git ${args[0]} failed: ${result.stderr.toString().trim()}`);
  return result.stdout.toString().trim();
}

try {
  if (process.env.GITHUB_ACTIONS !== "true") throw new Error("This script runs only in GitHub Actions. Use bun run release.");
  if (process.argv[2] === "prepare") {
    const sha = git("rev-parse", "HEAD");
    const expected = process.env.EXPECTED_SHA;
    if (expected && expected !== sha) throw new Error("The checkout does not match the workflow source commit. Run the release again.");
    const tags = git("tag", "--list", "v*", "--sort=-version:refname").split("\n")
      .filter((tag) => /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag));
    const previousTag = tags[0] ?? null;
    if (previousTag) git("merge-base", "--is-ancestor", previousTag, "HEAD");
    const pkg = JSON.parse(readFileSync("package.json", "utf8"));
    const range = previousTag ? `${previousTag}..HEAD` : "HEAD";
    const commits = git("log", "--reverse", "--format=%H%x09%s", range).split("\n").filter(Boolean)
      .map((line) => ({ sha: line.slice(0, 40), subject: line.slice(41) }));
    if (!commits.length) throw new Error("There are no changes since the previous release.");
    if (commits.length > maxCommits) throw new Error(`This release exceeds the ${maxCommits}-commit AI budget.`);
    const bump = releaseBump(git("log", "--format=%B%x00", range).split("\0").map((message) => message.trim()));
    const version = nextVersion(previousTag?.slice(1) ?? pkg.version, bump);
    const tag = `v${version}`;
    mkdirSync("dist", { recursive: true });
    writeFileSync("dist/release-context.json", JSON.stringify({ sha, version, tag, previousTag, commits }));
    // Stamp build inputs in the disposable CI checkout. Tags remain the version source of truth.
    pkg.version = version;
    writeFileSync("package.json", `${JSON.stringify(pkg, null, 2)}\n`);
    // Workspace members inherit the version from [workspace.package]; the lockfile lists each one.
    const manifest = readFileSync("Cargo.toml", "utf8");
    const stamped = manifest.replace(/(\[workspace\.package\]\nversion = ")[^"]+(")/, (_, prefix, suffix) => `${prefix}${version}${suffix}`);
    if (stamped === manifest && !manifest.includes(`version = "${version}"`)) throw new Error("Could not stamp [workspace.package] in Cargo.toml.");
    writeFileSync("Cargo.toml", stamped);
    const lock = readFileSync("Cargo.lock", "utf8");
    const lockPattern = /(name = "tendkit(?:-[\w]+)*"\nversion = ")[^"]+(")/g;
    if (!lockPattern.test(lock)) throw new Error("Could not stamp the TendKit packages in Cargo.lock.");
    writeFileSync("Cargo.lock", lock.replace(lockPattern, (_, prefix, suffix) => `${prefix}${version}${suffix}`));
    const plist = readFileSync("apps/tendkit/macos/Info.plist", "utf8")
      .replace(/(<key>CFBundleShortVersionString<\/key>\s*<string>)[^<]+/, (_, prefix) => `${prefix}${version}`)
      .replace(/(<key>CFBundleVersion<\/key>\s*<string>)[^<]+/, (_, prefix) => `${prefix}${process.env.GITHUB_RUN_NUMBER}`);
    writeFileSync("apps/tendkit/macos/Info.plist", plist);
    appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\ntag=${tag}\nsha=${sha}\n`);
    console.log(`Preparing ${tag} (${bump}) from ${commits.length} commits.`);
  } else if (process.argv[2] === "notes") {
    const context = JSON.parse(readFileSync("dist/release-context.json", "utf8"));
    const { items, usage } = await generateNotes(context.commits, process.env.OPENAI_API_KEY);
    writeFileSync("dist/RELEASE_NOTES.md", renderNotes({ ...context, items, repository: process.env.GITHUB_REPOSITORY }));
    if (usage) console.log(`AI tokens: input=${usage.input_tokens}, output=${usage.output_tokens}`);
  } else {
    throw new Error("Usage: bun scripts/release-ci.js [prepare|notes]");
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
