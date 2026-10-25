import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

function yaml(path) {
  return Bun.YAML.parse(readFileSync(new URL(`../${path}`, import.meta.url), "utf8"));
}

test("release builds wait for validation and publication waits for verified artifacts", () => {
  const { jobs } = yaml(".github/workflows/release.yml");
  const platforms = ["macos_arm64", "macos_intel", "windows", "linux"];
  expect(jobs.prepare.needs).toBe("validate");
  for (const id of platforms) {
    expect(jobs[id].needs).toBe("prepare");
    expect(jobs[id]["continue-on-error"] ?? false).toBe(false);
    expect(jobs[id].steps.some((step) => step.uses === "./.github/actions/build-release")).toBe(true);
  }
  expect(jobs.verify.needs.toSorted()).toEqual(platforms.toSorted());
  expect(jobs.publish.needs).toBe("verify");
  expect(jobs.verify.if).toBe("success()");
  expect(jobs.publish.if).toBe("success()");
  expect(jobs.publish.steps[0].with.name).toBe("verified-release");
  const publish = jobs.publish.steps.find((step) => step.run?.includes("gh release create")).run;
  expect(publish.indexOf("sha256sum --check")).toBeLessThan(publish.indexOf("gh release create"));
  expect(jobs.verify.steps.some((step) => step.run?.includes("package-release.js finalize"))).toBe(true);
});

test("shared release steps preserve platform tests, signing and engine validation", () => {
  const action = yaml(".github/actions/build-release/action.yml");
  const steps = action.runs.steps;
  expect(action.runs.using).toBe("composite");
  expect(steps.every((step) => !step.run || step.shell === "bash")).toBe(true);
  const signing = steps.findIndex((step) => step.run?.includes("notarize-app.sh"));
  const packaging = steps.findIndex((step) => step.run?.includes("package-release.js package"));
  expect(signing).toBeGreaterThan(-1);
  expect(signing).toBeLessThan(packaging);
  expect(steps[signing].if).toBe("inputs.os == 'macos'");
  expect(steps.some((step) => step.run?.includes("cargo test --locked -p tendkit"))).toBe(true);
  expect(steps.some((step) => step.run?.includes("smoke-engine.js"))).toBe(true);
  // Composite actions inherit credentials from the calling job.
  expect(JSON.stringify(action)).not.toContain("secrets.");
  const ci = yaml(".github/workflows/ci.yml");
  expect(Object.values(ci.jobs).every((job) => job["runs-on"] === "ubuntu-24.04")).toBe(true);
});
