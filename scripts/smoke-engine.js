import { resolve } from "node:path";

const name = process.platform === "win32" ? "tendkit-engine.exe" : "tendkit-engine";
const requestId = "release-smoke";
const request = { protocol_version: 1, request_id: requestId, command: "scan", selected: [], configure_shell: false };
const child = Bun.spawn([resolve(`target/${process.env.SMOKE_PROFILE ?? "release"}`, name)], { stdin: new TextEncoder().encode(`${JSON.stringify(request)}\n`), stdout: "pipe", stderr: "pipe" });
const timeout = setTimeout(() => child.kill(), 60000);
try {
  const [output, errors, status] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
  if (status !== 0) throw new Error(`Native engine failed: ${errors}`);
  const events = output.trim().split("\n").map((line) => JSON.parse(line));
  if (events.some((event) => event.request_id !== requestId || event.type === "error") || !events.some((event) => event.type === "hello") || !events.some((event) => event.type === "run_finished")) {
    throw new Error(`Native engine returned an incomplete response: ${output}`);
  }
  const detected = new Set(events.filter((event) => event.type === "tool_detected").map((event) => event.data.id));
  if (detected.size !== 9 || events.at(-1).type !== "run_finished" || events.at(-1).data.status !== "succeeded") throw new Error("Native engine did not complete the catalog scan.");
  console.log(`Native engine protocol verified on ${process.platform}/${process.arch}.`);
} finally {
  clearTimeout(timeout);
}
