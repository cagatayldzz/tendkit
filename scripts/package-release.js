import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync, chmodSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const releaseTargets = [
  { runner: "macos-26", os: "macos", arch: "arm64", suffixes: ["macos-arm64.zip"] },
  { runner: "macos-15-intel", os: "macos", arch: "x64", suffixes: ["macos-x64.zip"] },
  { runner: "windows-2025", os: "windows", arch: "x64", suffixes: ["windows-x64.zip"] },
  { runner: "ubuntu-24.04", os: "linux", arch: "x64", suffixes: ["linux-x64.tar.gz", "linux-x64.deb"] }
];

export function artifactNames(tag) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error("Invalid release tag.");
  return releaseTargets.flatMap((target) => target.suffixes.map((suffix) => `TendKit-${tag}-${suffix}`));
}

function run(args) {
  const result = Bun.spawnSync(args, { stdout: "inherit", stderr: "inherit" });
  if (result.exitCode !== 0) throw new Error(`${args[0]} failed (${result.exitCode}).`);
}

export function finalize(directory, tag) {
  const names = artifactNames(tag);
  const archives = readdirSync(directory).filter((name) => /\.(zip|tar\.gz|deb)$/.test(name));
  if (archives.length !== names.length || archives.some((name) => !names.includes(name))) throw new Error("Release assets do not match the required platform matrix.");
  const sums = names.map((name) => {
    const path = join(directory, name);
    if (!statSync(path).size) throw new Error(`Empty release asset: ${name}`);
    return `${createHash("sha256").update(readFileSync(path)).digest("hex")}  ${name}`;
  });
  writeFileSync(join(directory, "SHA256SUMS.txt"), `${sums.join("\n")}\n`);
  return names;
}

export function packageWindows(source, archive, execute = run) {
  if (!source || !archive) throw new Error("Windows packaging requires source and archive paths.");
  const script = fileURLToPath(new URL("package-windows.ps1", import.meta.url));
  execute(["powershell.exe", "-NoProfile", "-NonInteractive", "-File", script, "-SourcePath", source, "-ArchivePath", archive]);
  if (!existsSync(archive) || !statSync(archive).size) throw new Error("Windows packaging did not produce an archive.");
}

function packageNative(tag) {
  artifactNames(tag);
  const os = { darwin: "macos", win32: "windows", linux: "linux" }[process.platform];
  const target = releaseTargets.find((target) => target.os === os && target.arch === process.arch);
  if (!target) throw new Error(`Unsupported release host: ${process.platform}/${process.arch}`);
  mkdirSync("dist", { recursive: true });
  const base = `TendKit-${tag}-${os}-${process.arch}`;
  if (os === "macos") {
    if (!existsSync("dist/TendKit.app/Contents/Helpers/tendkit-engine")) throw new Error("Build and notarize the macOS application first.");
    run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", "dist/TendKit.app", `dist/${base}.zip`]);
    return;
  }
  const stage = resolve("dist/TendKit");
  mkdirSync(stage, { recursive: true });
  const extension = os === "windows" ? ".exe" : "";
  for (const name of os === "windows" ? ["TendKit"] : ["TendKit", "tendkit-engine"]) copyFileSync(`target/release/${name}${extension}`, join(stage, `${name}${extension}`));
  if (os !== "windows") copyFileSync("docs/platform-support.md", join(stage, "README.md"));
  if (os === "windows") {
    packageWindows(stage, resolve(`dist/${base}.zip`));
    return;
  }
  run(["tar", "-czf", `dist/${base}.tar.gz`, "-C", "dist", "TendKit"]);
  const deb = resolve("dist/deb");
  for (const path of ["DEBIAN", "opt/tendkit", "usr/bin", "usr/share/applications"]) mkdirSync(join(deb, path), { recursive: true });
  for (const name of ["TendKit", "tendkit-engine"]) {
    copyFileSync(join(stage, name), join(deb, "opt/tendkit", name));
    chmodSync(join(deb, "opt/tendkit", name), 0o755);
  }
  writeFileSync(join(deb, "usr/bin/tendkit"), '#!/bin/sh\nexec /opt/tendkit/TendKit "$@"\n');
  chmodSync(join(deb, "usr/bin/tendkit"), 0o755);
  writeFileSync(join(deb, "usr/share/applications/com.cagatayldzz.tendkit.desktop"), "[Desktop Entry]\nType=Application\nName=TendKit\nComment=Developer environment and disk inspection\nExec=tendkit\nIcon=utilities-terminal\nTerminal=false\nCategories=Development;Utility;\n");
  writeFileSync(join(deb, "DEBIAN/control"), `Package: tendkit\nVersion: ${tag.slice(1)}\nArchitecture: amd64\nMaintainer: TendKit\nSection: devel\nPriority: optional\nDepends: libc6 (>= 2.39), libstdc++6, libasound2t64, libfontconfig1, libfreetype6, libxkbcommon0, libxkbcommon-x11-0, libxcb1, libxcb-render0, libxcb-shape0, libxcb-xfixes0, libx11-6, libxi6, libegl1, libvulkan1, libwayland-client0, curl\nDescription: Developer environment and disk inspection (Linux preview)\n`);
  run(["dpkg-deb", "--build", "--root-owner-group", deb, `dist/${base}.deb`]);
}

if (import.meta.main) {
  try {
    const tag = process.env.RELEASE_TAG;
    if (process.argv[2] === "finalize") finalize("dist", tag);
    else if (process.argv[2] === "package") {
      packageNative(tag);
    } else throw new Error("Usage: bun scripts/package-release.js [package|finalize]");
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
