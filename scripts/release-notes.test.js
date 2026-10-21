import { describe, expect, test } from "bun:test";
import { generateNotes, maxCommits, nextVersion, releaseBump, renderNotes, validateItems } from "./release-notes.js";

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

test("AI notes cannot omit or duplicate commits or invent references", () => {
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

test("one bounded API call sends titles and parses structured output", async () => {
  let calls = 0;
  const fetcher = async (url, options) => {
    calls += 1;
    expect(url).toBe("https://api.openai.com/v1/responses");
    const body = JSON.parse(options.body);
    expect(body.model).toBe("gpt-4o-mini");
    expect(body.max_output_tokens).toBe(2400);
    expect(body.store).toBe(false);
    expect(body.text.format.strict).toBe(true);
    expect(JSON.parse(body.input)).toEqual(commits.map((commit, id) => ({ id, title: commit.subject })));
    return Response.json({
      status: "completed",
      output: [{ content: [{ type: "output_text", text: JSON.stringify({ items }) }] }],
      usage: { input_tokens: 120, output_tokens: 60 }
    });
  };
  expect((await generateNotes(commits, "test-key", fetcher)).items).toEqual(items);
  expect(calls).toBe(1);
});

test("missing secrets, empty changes and oversized releases fail before billing", async () => {
  const neverFetch = () => { throw new Error("Must not call the API"); };
  await expect(generateNotes(commits, "", neverFetch)).rejects.toThrow("OPENAI_API_KEY");
  await expect(generateNotes([], "test-key", neverFetch)).rejects.toThrow("no changes");
  await expect(generateNotes(Array(maxCommits + 1).fill(commits[0]), "test-key", neverFetch))
    .rejects.toThrow("limited");
});

test("API failures and truncated output stop release generation", async () => {
  await expect(generateNotes(commits, "test-key", async () => new Response("sensitive details", { status: 429 })))
    .rejects.toThrow("HTTP 429");
  await expect(generateNotes(commits, "test-key", async () => Response.json({ status: "incomplete" })))
    .rejects.toThrow("did not complete");
});
