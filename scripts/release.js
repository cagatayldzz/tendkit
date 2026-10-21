import { fileURLToPath } from "node:url";

process.chdir(fileURLToPath(new URL("..", import.meta.url)));

function run(args) {
  const result = Bun.spawnSync(args, { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) {
    throw new Error(result.stderr.toString().trim() || `${args[0]} failed.`);
  }
  return result.stdout.toString().trim();
}

try {
  if (process.argv.length > 2) {
    throw new Error("Usage: bun run release (version increment is automatic)");
  }
  if (!Bun.which("gh")) {
    throw new Error("Install GitHub CLI (brew install gh), then run gh auth login.");
  }
  if (run(["git", "status", "--porcelain"])) {
    throw new Error("Commit and push all changes before releasing.");
  }
  const repository = run(["gh", "repo", "view", "--json", "nameWithOwner,defaultBranchRef"]);
  const { nameWithOwner, defaultBranchRef } = JSON.parse(repository);
  const branch = defaultBranchRef.name;
  if (run(["git", "branch", "--show-current"]) !== branch) {
    throw new Error(`Release from the default branch: ${branch}.`);
  }
  const sha = run(["git", "rev-parse", "HEAD"]);
  const remoteSHA = run(["gh", "api", `repos/${nameWithOwner}/commits/${branch}`, "--jq", ".sha"]);
  if (sha !== remoteSHA) {
    throw new Error("Local HEAD must match the pushed default branch. Push or pull before releasing.");
  }
  run(["gh", "workflow", "run", "release.yml", "--repo", nameWithOwner, "--ref", branch]);
  console.log(`Release requested from ${branch}. The version increment is automatic.`);
  console.log(`Follow progress: https://github.com/${nameWithOwner}/actions/workflows/release.yml`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
