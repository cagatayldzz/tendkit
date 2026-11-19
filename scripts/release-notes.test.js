import { describe, expect, test } from "bun:test";
import { classifyCommits, nextVersion, noteSection, releaseBump, renderNotes, validateItems } from "./release-notes.js";

const commits = [
  { sha: "a".repeat(40), subject: "feat: add native window (#12)" },
  { sha: "b".repeat(40), subject: "fix: preserve engine output" }
];
const items = [
  { id: 0, section: "New Features", summary: "Add a native window (#12)." },
  { id: 1, section: "Bug Fixes", summary: "Preserve engine output." }
];

describe("release version selection", () => {
  test("selects the highest increment from commit subjects and breaking change footers", () => {
    expect(releaseBump(["fix: preserve output", "chore: update CI"])).toBe("patch");
    expect(releaseBump(["Update documentation"])).toBe("patch");
    expect(releaseBump(["fix: preserve output", "feat(app): add preferences"])).toBe("minor");
    expect(releaseBump(["feat: add preferences", "refactor(engine)!: replace protocol"])).toBe("major");
    expect(releaseBump(["feat!: replace protocol"])).toBe("major");
    expect(releaseBump(["fix: update protocol\n\nBREAKING CHANGE: remove legacy commands"])).toBe("major");
    expect(releaseBump(["fix: update protocol\n\nBREAKING-CHANGE: remove legacy commands"])).toBe("major");
  });

  test("increments stable semantic versions", () => {
    expect(nextVersion("0.2.9", "patch")).toBe("0.2.10");
    expect(nextVersion("0.2.9", "minor")).toBe("0.3.0");
    expect(nextVersion("0.2.9", "major")).toBe("1.0.0");
    expect(() => nextVersion("0.2.9-beta", "patch")).toThrow();
    expect(() => nextVersion("0.2.9", "invalid")).toThrow();
  });
});

test("notes cannot omit or duplicate commits or invent references", () => {
  expect(() => validateItems(items.slice(0, 1), 2)).toThrow();
  expect(() => validateItems([items[0], items[0]], 2)).toThrow();
  expect(() => validateItems([items[0], { ...items[1], id: 9 }], 2)).toThrow();
  expect(() => validateItems([{ ...items[0], section: "Other" }], 1)).toThrow();
  expect(() => validateItems([{ ...items[0], summary: "Title\n## Injected" }], 1)).toThrow();
});

test("notes contain every section, source links and a complete changelog", () => {
  const markdown = renderNotes({ items, commits, repository: "owner/repo", tag: "v0.3.0", previousTag: "v0.2.0" });
  for (const section of ["New Features", "Bug Fixes", "Documentation", "Chores", "Changelog"]) {
    expect(markdown).toContain(`## ${section}\n`);
  }
  expect(markdown).toContain("https://github.com/owner/repo/compare/v0.2.0...v0.3.0");
  for (const commit of commits) expect(markdown).toContain(`/commit/${commit.sha}`);
  expect(renderNotes({ items, commits, repository: "owner/repo", tag: "v0.3.0", previousTag: null }))
    .toContain("https://github.com/owner/repo/commits/v0.3.0");
});

test("commit titles are grouped by their type and kept as written", () => {
  expect(noteSection("feat: add native window")).toBe("New Features");
  expect(noteSection("feat(app)!: replace protocol")).toBe("New Features");
  expect(noteSection("fix: preserve output")).toBe("Bug Fixes");
  expect(noteSection("docs: explain releases")).toBe("Documentation");
  expect(noteSection("refactor: split engine")).toBe("Chores");
  expect(noteSection("Update the readme")).toBe("Chores");
  expect(classifyCommits(commits)).toEqual([
    { id: 0, section: "New Features", summary: "feat: add native window (#12)" },
    { id: 1, section: "Bug Fixes", summary: "fix: preserve engine output" }
  ]);
});

test("a release with no commits is refused, and any number of commits is accepted", () => {
  expect(() => classifyCommits([])).toThrow("no changes");
  expect(classifyCommits(Array(200).fill(commits[0]))).toHaveLength(200);
});
