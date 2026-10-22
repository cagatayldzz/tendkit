import { readdirSync, readFileSync } from "node:fs";
import { posix, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export type Manifest = {
  package?: { autotests?: boolean };
  test?: { name: string; path?: string }[];
};

/** Separate Rust items, including methods, while leaving attributes attached. */
export function validateRustSpacing(source: string): number[] {
  const lines = source.split("\n");
  const boundaries: number[] = [];
  const previousItem = /^(\s*)(?:\};?|(?:use|fn|const|static|type|extern)\b.*;)$/;
  const nextItem = /^(\s*)(?:#\[|(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:fn|struct|enum|impl|trait)\b)/;
  for (let index = 1; index < lines.length; index++) {
    const previous = previousItem.exec(lines[index - 1]);
    const next = nextItem.exec(lines[index]);
    if (previous && next && previous[1] === next[1]) boundaries.push(index + 1);
  }
  return boundaries;
}

/** Paths are relative to one workspace package and use forward slashes. */
export function validateTestLayout(files: Record<string, string>, manifest: Manifest): string[] {
  const errors: string[] = [];
  const unitModules = new Set<string>();
  const integrationTests = new Set((manifest.test ?? []).map(test => test.path ?? `tests/${test.name}.rs`));
  const testAttribute = /#\s*\[\s*(?:[a-zA-Z_][\w]*::)*test\s*(?:\]|\()/;

  for (const [path, source] of Object.entries(files)) {
    if (!path.endsWith(".rs")) continue;
    for (const line of validateRustSpacing(source)) {
      errors.push(`${path}:${line}: add a blank line after imports and between Rust items.`);
    }
    if (!path.startsWith("src/")) continue;
    if (testAttribute.test(source) || /\bmod\s+tests\s*\{/.test(source)
      || /(?:^|\/)tests?(?:\/|\.rs$)/.test(path)) {
      errors.push(`${path}: move test implementations to tests/ with a descriptive file name.`);
    }
    for (const match of source.matchAll(/#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]/g)) {
      const target = posix.normalize(posix.join(posix.dirname(path), match[1]));
      if (!target.startsWith("tests/")) continue;
      unitModules.add(target);
      if (!(target in files)) errors.push(`${path}: test module ${target} does not exist.`);
    }
  }

  for (const path of integrationTests) {
    if (!path.startsWith("tests/") || posix.normalize(path) !== path) {
      errors.push(`${path}: integration test targets must live inside tests/.`);
    } else if (!(path in files)) {
      errors.push(`${path}: integration test target does not exist.`);
    }
  }

  for (const [path, source] of Object.entries(files)) {
    if (!path.startsWith("tests/") || !path.endsWith(".rs")) continue;
    if (/(?:^|\/)tests?\.rs$/.test(path)) {
      errors.push(`${path}: use a descriptive test file name, such as detection.rs.`);
    }
    const automatic = manifest.package?.autotests !== false
      && (/^tests\/[^/]+\.rs$/.test(path) || /^tests\/[^/]+\/main\.rs$/.test(path));
    if (unitModules.has(path) && (automatic || integrationTests.has(path))) {
      errors.push(`${path}: a unit test module must not also be an integration test target; set package.autotests = false and register integration targets explicitly.`);
    }
    if (testAttribute.test(source) && !unitModules.has(path) && !automatic && !integrationTests.has(path)) {
      errors.push(`${path}: tests are not registered; add a unit module path or an explicit integration test target.`);
    }
  }
  return errors;
}

function rustFiles(root: string, directory: string, files: Record<string, string>) {
  for (const entry of readdirSync(resolve(root, directory), { withFileTypes: true })) {
    const path = posix.join(directory, entry.name);
    if (entry.isDirectory()) rustFiles(root, path, files);
    else if (entry.name.endsWith(".rs")) files[path] = readFileSync(resolve(root, path), "utf8");
  }
}

if (import.meta.main) {
  const root = fileURLToPath(new URL("../", import.meta.url));
  const errors: string[] = [];
  for (const parent of ["apps", "crates"]) {
    for (const entry of readdirSync(resolve(root, parent), { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const directory = `${parent}/${entry.name}`;
      const packageRoot = resolve(root, directory);
      if (!await Bun.file(resolve(packageRoot, "Cargo.toml")).exists()) continue;
      const manifest = Bun.TOML.parse(readFileSync(resolve(packageRoot, "Cargo.toml"), "utf8")) as Manifest;
      const files: Record<string, string> = {};
      for (const folder of ["src", "tests"]) {
        if (readdirSync(packageRoot).includes(folder)) rustFiles(packageRoot, folder, files);
      }
      errors.push(...validateTestLayout(files, manifest).map(error => `${directory}/${error}`));
    }
  }
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log("Workspace test layout verified.");
}
