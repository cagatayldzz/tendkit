export const sections = ["New Features", "Bug Fixes", "Documentation", "Chores"];

export function releaseBump(messages) {
  if (messages.some((message) => /^[a-z]+(?:\([^\r\n]+\))?!:/i.test(message)
    || /^BREAKING[ -]CHANGE:\s/m.test(message))) return "major";
  if (messages.some((message) => /^feat(?:\([^\r\n]+\))?:/i.test(message))) return "minor";
  return "patch";
}

export function nextVersion(current, bump) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(current)) {
    throw new Error(`Invalid stable version: ${current}`);
  }
  const [major, minor, patch] = current.split(".").map(Number);
  switch (bump) {
    case "major": return `${major + 1}.0.0`;
    case "minor": return `${major}.${minor + 1}.0`;
    case "patch": return `${major}.${minor}.${patch + 1}`;
    default: throw new Error("Release bump must be patch, minor, or major.");
  }
}

// The section a commit title belongs to, from its Conventional Commits type.
export function noteSection(subject) {
  if (/^feat(?:\(|:|!)/i.test(subject)) return "New Features";
  if (/^fix(?:\(|:|!)/i.test(subject)) return "Bug Fixes";
  if (/^docs(?:\(|:|!)/i.test(subject)) return "Documentation";
  return "Chores";
}

// One note per commit, written from its title: nothing is sent anywhere and nothing is invented.
export function classifyCommits(commits) {
  if (!commits.length) throw new Error("There are no changes to release.");
  return commits.map((commit, id) => ({
    id,
    section: noteSection(commit.subject),
    summary: commit.subject.replace(/[\r\n]+/g, " ").trim().slice(0, 300) || "No title."
  }));
}

export function validateItems(items, count) {
  const ids = new Set();
  if (!Array.isArray(items) || items.length !== count) throw new Error("Notes must cover every commit exactly once.");
  for (const item of items) {
    if (!Number.isInteger(item.id) || item.id < 0 || item.id >= count || ids.has(item.id)
      || !sections.includes(item.section) || typeof item.summary !== "string"
      || !item.summary.trim() || item.summary.length > 300 || /[\r\n]/.test(item.summary)) {
      throw new Error("Notes contain invalid, duplicate, or missing commit references.");
    }
    ids.add(item.id);
  }
}

function plain(text) {
  return text.replace(/[\\`*_{}\[\]<>#!|]/g, "\\$&").replace(/[\r\n]/g, " ");
}

export function renderNotes({ items, commits, repository, tag, previousTag }) {
  validateItems(items, commits.length);
  const base = `https://github.com/${repository}`;
  const lines = [];
  for (const section of sections) {
    lines.push(`## ${section}`, "");
    const entries = items.filter((item) => item.section === section);
    lines.push(...(entries.length ? entries.map((item) =>
      `- ${plain(item.summary)} ([${commits[item.id].sha.slice(0, 7)}](${base}/commit/${commits[item.id].sha}))`)
      : ["- No changes."]), "");
  }
  lines.push("## Changelog", "");
  if (previousTag) {
    lines.push(`[Full Changelog: ${previousTag}...${tag}](${base}/compare/${previousTag}...${tag})`, "");
  } else {
    lines.push(`[Full Changelog](${base}/commits/${tag})`, "");
  }
    lines.push(...commits.map((commit) => `- [${commit.sha.slice(0, 7)}](${base}/commit/${commit.sha}) ${plain(commit.subject)}`), "");
  return lines.join("\n");
}
