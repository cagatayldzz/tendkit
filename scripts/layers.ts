import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

type Dependency = {
  name: string;
  kind: "dev" | "build" | null;
  path?: string;
  source: string | null;
  target?: string | null;
};

export type Metadata = {
  workspace_members: string[];
  packages: {
    id: string;
    name: string;
    manifest_path: string;
    dependencies: Dependency[];
    /** `[package.metadata.layers]` (Cargo.toml): the dependencies this package may use. */
    metadata?: { layers?: RawRule } | null;
  }[];
};

type RawRule = {
  dependencies?: string[];
  "dev-dependencies"?: string[];
  "build-dependencies"?: string[];
};

export type Policy = Record<string, {
  dependencies: string[];
  devDependencies?: string[];
  buildDependencies?: string[];
}>;

/** Builds the allow-list from each workspace member's `[package.metadata.layers]` table; a package without a table has no rule and fails the check. */
export function policyFromMetadata(metadata: Metadata): Policy {
  const members = new Set(metadata.workspace_members);
  const rules: Policy = {};
  for (const pkg of metadata.packages) {
    const raw = members.has(pkg.id) ? pkg.metadata?.layers : undefined;
    if (!raw) continue;
    rules[pkg.name] = {
      dependencies: raw.dependencies ?? [],
      devDependencies: raw["dev-dependencies"],
      buildDependencies: raw["build-dependencies"],
    };
  }
  return rules;
}

export function validateLayers(metadata: Metadata, rules: Policy): string[] {
  const members = new Set(metadata.workspace_members);
  const packages = metadata.packages.filter(pkg => members.has(pkg.id));
  const byName = new Map(packages.map(pkg => [pkg.name, pkg]));
  const edges = new Map<string, string[]>();
  const errors: string[] = [];

  for (const name of Object.keys(rules)) {
    if (!byName.has(name)) errors.push(`${name}: is in the policy but not in the workspace.`);
  }
  for (const pkg of packages) {
    const rule = rules[pkg.name];
    if (!rule) {
      errors.push(`${pkg.name}: has no layer rule.`);
      continue;
    }
    edges.set(pkg.name, []);
    // Cargo metadata also includes dev/build, aliased, and target-specific dependencies.
    for (const dep of pkg.dependencies) {
      const allowed = dep.kind === "dev"
        ? [...rule.dependencies, ...(rule.devDependencies ?? [])]
        : dep.kind === "build" ? (rule.buildDependencies ?? []) : rule.dependencies;
      if (!allowed.includes(dep.name)) {
        errors.push(`${pkg.name} → ${dep.name}: ${dep.kind ?? "normal"} dependency is not allowed.`);
      }
      if (dep.path) {
        const target = byName.get(dep.name);
        if (!target || resolve(dep.path) !== resolve(dirname(target.manifest_path))) {
          errors.push(`${pkg.name} → ${dep.name}: local dependency must be a workspace member.`);
        } else {
          edges.get(pkg.name)!.push(dep.name);
        }
      }
    }
  }

  const visited = new Set<string>();
  const active = new Set<string>();
  function visit(name: string) {
    if (active.has(name)) {
      errors.push(`${name}: workspace dependency cycle found.`);
      return;
    }
    if (visited.has(name)) return;
    active.add(name);
    for (const target of edges.get(name) ?? []) visit(target);
    active.delete(name);
    visited.add(name);
  }
  for (const name of edges.keys()) visit(name);
  return errors;
}

if (import.meta.main) {
  const result = Bun.spawnSync(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], {
    cwd: fileURLToPath(new URL("../", import.meta.url)),
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) {
    console.error(result.stderr.toString());
    process.exit(result.exitCode || 1);
  }
  const metadata: Metadata = JSON.parse(result.stdout.toString());
  const errors = validateLayers(metadata, policyFromMetadata(metadata));
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log("Workspace dependency boundaries verified.");
}
