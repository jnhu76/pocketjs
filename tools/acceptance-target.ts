// The acceptance rig's test-local provisional target profile for W1-G.
//
// `windows-app` is a PROPOSAL (docs/windows-desktop-w1-report.md §16.5), not
// a registered contract: POCKET_TARGETS inventories real, golden-tested
// stock hosts only, and nothing here may leak into it. This registry exists
// for one reason — pairing. A plan-built bundle asserts its plan's
// (target.id, hostAbi) against the host's --identity/--host-abi
// (framework/src/host.ts rejects any mismatch), so the acceptance bundle
// and the runtime host must agree on ONE identity. Building the bundle as
// `macos-widget` and running the host as `windows-app` is not a legal
// validation setup; this profile makes both sides `windows-app`.
//
// hostAbi 3 is provisional — the flat host speaks the same wire generation
// as macos-widget; whether windows-app gets its own number is the upstream
// W1-G discussion. Capabilities are the measured candidate set the W1
// mechanisms actually deliver, not product wishes.
import {
  POCKET_CAPABILITIES,
  POCKET_TARGETS,
  definePlatformContractRegistry,
  type PocketCapabilityId,
  type TargetProfile,
} from "../contracts/spec/platforms.ts";

export const ACCEPTANCE_TARGET_ID = "windows-app";
export const ACCEPTANCE_HOST_ABI = 3;

const WINDOWS_APP: TargetProfile<PocketCapabilityId> = {
  hostAbi: ACCEPTANCE_HOST_ABI,
  platform: "windows",
  form: "window",
  display: {
    physicalViewport: [2000, 1400],
    logicalViewports: [[1000, 700]],
    dynamicViewport: { min: [240, 180], max: [4096, 4096] },
    presentations: ["native"],
    rasterDensity: 2,
  },
  capabilities: [
    "input.buttons",
    "input.ime",
    "input.pointer",
    "input.text",
    "host.clipboard",
    "display.viewport.live",
    "text.glyphs.baked",
    "text.glyphs.runtime",
  ],
};

/** POCKET_TARGETS plus the provisional windows-app profile — acceptance
 *  builds only; never a substitute for the formal registry. */
export const ACCEPTANCE_REGISTRY = definePlatformContractRegistry(POCKET_CAPABILITIES, {
  ...POCKET_TARGETS,
  [ACCEPTANCE_TARGET_ID]: WINDOWS_APP,
});
