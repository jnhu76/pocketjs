// bun run acceptance [flags…] — build + headless-prove the generic desktop
// acceptance surface (apps/acceptance over the flat pocket-widget host).
//
// The acceptance app is the W1 gate surface: a deliberately plain text
// editor with no Markit/markdown product logic. If it fails, PocketJS or
// the platform substrate failed — not a product parser.
//
//   bun run acceptance                        # build + headless proof
//   bun run acceptance -- --width 900 --height 600
//
// Identity pairing: the bundle builds against the provisional windows-app
// target (tools/acceptance-target.ts) so its plan asserts exactly what the
// windowed host announces (--identity windows-app --host-abi 3). Building
// against macos-widget while running as windows-app would fail pairing.
//
// Evidence semantics — what this headless run is and is not:
//
//   DESKTOP_WIRE_ACCEPTANCE (this run): --type/--paste/--key inject svc
//     lines directly. This proves the svc protocol, the guest editing
//     surface and DrawList rendering. It does NOT exercise the OS input
//     stack (winit → Input → forward_edits/forward_ime → primary chords),
//     and --paste is injected text, not a system clipboard read.
//   PLATFORM_INTEGRATION_ACCEPTANCE (windowed, on the target OS): the real
//     keyboard/IME/clipboard path — docs/windows-desktop-w1-report.md.
//
// The W1 validation charset is ASCII + BMP CJK; astral codepoints (emoji,
// CJK Extension B) hit the known UTF-16/scalar indexing seam and are a
// recorded follow-up, not part of this proof.
//
// The screenshot lands in dist/acceptance-proof.png.
import { fsPath } from "./fs-url.ts";
import { ACCEPTANCE_HOST_ABI, ACCEPTANCE_REGISTRY, ACCEPTANCE_TARGET_ID } from "./acceptance-target.ts";
import { mkdirSync } from "node:fs";
import { resolve as resolvePath } from "node:path";
import { $ } from "bun";
import { validateAndResolveBuildPlan } from "../framework/src/manifest/resolve.ts";

const root = fsPath("..", import.meta.url);
const args = process.argv.slice(2).filter((a) => a !== "--");
const pass = args;

const manifest = await Bun.file(`${root}apps/acceptance/pocket.json`).json();
const resolution = validateAndResolveBuildPlan(
  manifest,
  { target: ACCEPTANCE_TARGET_ID },
  ACCEPTANCE_REGISTRY,
);
if (!resolution.ok) {
  throw new Error(
    `pocket-acceptance: manifest did not resolve: ${resolution.diagnostics
      .map((d) => `${d.path || "/"}: ${d.message}`)
      .join("; ")}`,
  );
}

const planPath = `${root}.pocket/windows-app/acceptance-main.plan.json`;
mkdirSync(resolvePath(planPath, ".."), { recursive: true });
await Bun.write(planPath, JSON.stringify(resolution.plan, null, 2) + "\n");

await $`bun tools/build.ts --plan=${planPath} --project-root=${root}`.cwd(root);
await $`cargo build -p note-widget`.cwd(`${root}engine`);

const bin = `${root}engine/target/debug/note-widget`;
const env = { ...process.env, RUST_LOG: process.env.RUST_LOG ?? "info" };
const shot = `${root}dist/acceptance-proof.png`;

await $`rm -f ${shot}`;
// --form window: the generic desktop host posture (opaque, decorated,
// normal level, no note file/save/menu/drag/grip), so even headless the
// wire behavior is the acceptance one — no ~/.pocket-note.md can leak in.
// --identity/--host-abi pair the host with the plan target the bundle was
// built for; any drift fails boot by design (framework/src/host.ts).
await $`${bin} --app acceptance-main --form window --identity ${ACCEPTANCE_TARGET_ID} --host-abi ${ACCEPTANCE_HOST_ABI} --width 1000 --height 700 --screenshot ${shot} --frames 90 --type "ACCEPT-RUN-OK@10" --paste "PASTED@20" --key "Copy@30" ${pass}`.env(
  env,
);

console.log(
  "\nDESKTOP_WIRE_ACCEPTANCE: plan-built generic text surface (windows-app," +
    `\n  hostAbi ${ACCEPTANCE_HOST_ABI}) booted headless; scripted type + paste +` +
    "\n  copy ran through the svc wire. OS keyboard/IME/clipboard remain" +
    "\n  PLATFORM_INTEGRATION_ACCEPTANCE (windowed run on the target OS)." +
    `\n${shot}`,
);
