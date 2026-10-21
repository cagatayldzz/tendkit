import { expect, test } from "bun:test";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

test("notarization gates distribution and removes the temporary signing keychain", () => {
  const root = mkdtempSync(join(tmpdir(), "tendkit-notarization-"));
  try {
    for (const path of ["scripts", "bin", "dist/TendKit.app", "temp"]) {
      mkdirSync(join(root, path), { recursive: true });
    }
    copyFileSync(new URL("notarize-app.sh", import.meta.url), join(root, "scripts/notarize-app.sh"));
    const mock = `#!/bin/bash
set -eu
command_name="\${0##*/}"
printf '%s\\n' "$command_name $*" >> "$COMMAND_LOG"
case "$command_name" in
  security)
    if [[ "$1" == list-keychains ]]; then
      if [[ "$#" == 3 ]]; then
        echo '    "/tmp/Original Keychains/login.keychain-db"'
      elif [[ "$5" == */signing.keychain-db ]]; then
        printf '%s' "$5" > "$COMMAND_LOG.active-keychain"
      fi
    fi
    if [[ "$1" == find-identity && "\${INVALID_IDENTITY:-}" != true ]]; then
      echo '1) AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "Developer ID Application: Test (TESTTEAM)"'
    fi
    ;;
  codesign)
    if [[ "$1" != --verify && ! -f "$COMMAND_LOG.active-keychain" ]]; then
      echo 'error: The specified item could not be found in the keychain.' >&2
      exit 1
    fi
    ;;
  xcrun)
    if [[ "$1 $2" == 'notarytool submit' ]]; then
      printf '{"status":"%s"}\\n' "$NOTARY_STATUS"
    fi
    ;;
  spctl) exit "\${ASSESSMENT_EXIT:-0}" ;;
esac
`;
    for (const command of ["security", "curl", "codesign", "ditto", "xcrun", "spctl"]) {
      const path = join(root, "bin", command);
      writeFileSync(path, mock);
      chmodSync(path, 0o755);
    }
    const log = join(root, "commands.log");
    const run = (overrides = {}) => {
      writeFileSync(log, "");
      rmSync(`${log}.active-keychain`, { force: true });
      const result = Bun.spawnSync(["bash", "scripts/notarize-app.sh"], {
        cwd: root,
        env: {
          ...process.env,
          PATH: `${join(root, "bin")}:${process.env.PATH}`,
          RUNNER_TEMP: join(root, "temp"),
          COMMAND_LOG: log,
          APPLE_CERTIFICATE_BASE64: "dGVzdA==",
          APPLE_CERTIFICATE_PASSWORD: "fixture-password",
          APPLE_ID: "test@example.com",
          APPLE_TEAM_ID: "TESTTEAM",
          APPLE_APP_SPECIFIC_PASSWORD: "fixture-app-password",
          NOTARY_STATUS: "Accepted",
          ...overrides
        },
        stdout: "pipe",
        stderr: "pipe"
      });
      return { ...result, commands: readFileSync(log, "utf8") };
    };
    const accepted = run();
    expect(accepted.exitCode).toBe(0);
    expect(accepted.commands).toContain("--options runtime --timestamp");
    expect(accepted.commands.indexOf("Contents/Helpers/tendkit-engine"))
      .toBeLessThan(accepted.commands.indexOf("codesign --verify"));
    expect(accepted.commands.indexOf("notarytool submit")).toBeLessThan(accepted.commands.indexOf("stapler staple"));
    expect(accepted.commands).toContain("stapler validate");
    expect(accepted.commands).toContain("spctl --assess");
    expect(accepted.commands).toContain("security delete-keychain");
    expect(accepted.commands).toContain("security list-keychains -d user -s /tmp/Original Keychains/login.keychain-db");
    const rejected = run({ NOTARY_STATUS: "Invalid" });
    expect(rejected.exitCode).not.toBe(0);
    expect(rejected.commands).not.toContain("stapler staple");
    expect(rejected.commands).toContain("security delete-keychain");
    expect(run({ ASSESSMENT_EXIT: "1" }).exitCode).not.toBe(0);
    const invalid = run({ INVALID_IDENTITY: "true" });
    expect(invalid.exitCode).not.toBe(0);
    expect(invalid.commands).not.toContain("notarytool submit");
    const missing = run({ APPLE_APP_SPECIFIC_PASSWORD: "" });
    expect(missing.exitCode).not.toBe(0);
    expect(missing.commands).toBe("");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
