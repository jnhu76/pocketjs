import { describe, expect, test } from "bun:test";
import { POCKET_TARGETS } from "../contracts/spec/platforms.ts";
import { validateAndResolveBuildPlan } from "../framework/src/manifest/resolve.ts";
import {
  ACCEPTANCE_HOST_ABI,
  ACCEPTANCE_REGISTRY,
  ACCEPTANCE_TARGET_ID,
} from "../tools/acceptance-target.ts";

// The acceptance rig's provisional windows-app target: it exists ONLY so
// the acceptance bundle and the runtime host pair on one identity
// (plan target.id === host --identity). It must never leak into the formal
// registry while W1-G is still a proposal.
const manifest: unknown = await Bun.file(
  new URL("../apps/acceptance/pocket.json", import.meta.url),
).json();

describe("acceptance provisional windows-app target", () => {
  test("stays out of the formal registry while W1-G is a proposal", () => {
    expect(ACCEPTANCE_TARGET_ID).toBe("windows-app");
    expect(Object.keys(POCKET_TARGETS)).not.toContain(ACCEPTANCE_TARGET_ID);
  });

  test("resolves the acceptance app to a plan identity the host can pair on", () => {
    const result = validateAndResolveBuildPlan(
      manifest,
      { target: ACCEPTANCE_TARGET_ID },
      ACCEPTANCE_REGISTRY,
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    // Exactly what note-widget announces via --identity/--host-abi: a
    // bundle built as macos-widget here would fail pairing at boot.
    expect(result.plan.target).toEqual({ id: ACCEPTANCE_TARGET_ID, hostAbi: ACCEPTANCE_HOST_ABI });
    // Every capability the acceptance surface enhances is available.
    for (const capability of ["input.ime", "input.pointer", "host.clipboard", "text.glyphs.runtime"]) {
      expect(result.plan.features[capability]).toBe(true);
    }
  });

  test("keeps the formal targets resolvable unchanged alongside it", () => {
    expect(
      validateAndResolveBuildPlan(manifest, { target: "macos-widget" }, ACCEPTANCE_REGISTRY).ok,
    ).toBe(true);
  });
});
