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
// The proof drives scripted input through the real host input path:
//   --type "…@10"   typed chars  → svc ch line
//   --paste "…@20"  paste        → host reads clipboard (pocket-clipboard)
//   --key "Copy@30" named key    → app sends {t:"copy"} → host logs it
// The screenshot lands in dist/acceptance-proof.png.
//
// The bundle builds against the macos-widget target profile (the only
// desktop contract registered today) so plan features (input.text,
// input.ime, host.clipboard, text.glyphs.runtime, …) are baked. A
// provisional windows-app profile is the W1-G proposal, not yet a
// registered contract — see the W1 report.
import { fsPath } from "./fs-url.ts";
import { mkdirSync } from "node:fs";
import { resolve as resolvePath } from "node:path";
import { $ } from "bun";
import { validateAndResolveBuildPlan } from "../framework/src/manifest/resolve.ts";

const root = fsPath("..", import.meta.url);
const args = process.argv.slice(2).filter((a) => a !== "--");
const pass = args;

const manifest = await Bun.file(`${root}apps/acceptance/pocket.json`).json();
const resolution = validateAndResolveBuildPlan(manifest, { target: "macos-widget" });
if (!resolution.ok) {
  throw new Error(
    `pocket-acceptance: manifest did not resolve: ${resolution.diagnostics
      .map((d) => `${d.path || "/"}: ${d.message}`)
      .join("; ")}`,
  );
}
const planPath = `${root}.pocket/desktop-widget/acceptance-main.plan.json`;
mkdirSync(resolvePath(planPath, ".."), { recursive: true });
await Bun.write(planPath, JSON.stringify(resolution.plan, null, 2) + "\n");

await $`bun tools/build.ts --plan=${planPath} --project-root=${root}`.cwd(root);
await $`cargo build -p note-widget`.cwd(`${root}engine`);

const bin = `${root}engine/target/debug/note-widget`;
const env = { ...process.env, RUST_LOG: process.env.RUST_LOG ?? "info" };
const shot = `${root}dist/acceptance-proof.png`;

await $`rm -f ${shot}`;
await $`${bin} --app acceptance-main --width 1000 --height 700 --screenshot ${shot} --frames 90 --type "ACCEPT-RUN-OK@10" --paste "PASTED@20" --key "Copy@30" ${pass}`.env(
  env,
);

console.log(
  "\nacceptance: plan-built generic text surface booted headless; scripted" +
    "\ntype + paste + copy ran through the host input path." +
    `\n${shot}`,
);
