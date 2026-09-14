# PocketJS Phase-A Preservation Record (2026-09-14)

This branch is a documentation-only preservation branch on the personal fork
`jnhu76/pocketjs`. It is NOT a product branch. It must never be merged into
`main` or submitted upstream to `pocket-stack/pocketjs`.

## What happened

The Architecture Phase A PocketJS work for PicoView (product chains A1-A7 and
C1-C4, plus the Windows startup measurement/audit campaigns) was developed on
local-only branches and worktrees under `C:\Users\fred1\source`. On
2026-09-14 that entire local topology was preserved and retired
(POCKETJS-PICOVIEW-PHASE-A-PRESERVATION-1, followed by
POCKETJS-LOCAL-LEGACY-CLEANOUT-AND-FRESH-FORK-BOOTSTRAP-1).

## Where the history lives

- Git refs: annotated tags `archive/picoview-20260914/*` in this fork
  (21 tags: 14 branch tips, the frozen baseline alias, the product boundary
  marker `last-clean-product-tip`, and 5 fsck-recovered dropped-stash /
  superseded-draft commits).
- This branch: `docs/archive/picoview-20260914/MANIFEST.md` is the full
  manifest (commit chain, classifications, bundle hashes, verification
  results, chain of custody).

## Key identities

- Frozen campaign baseline (upstream main at preservation time):
  `a5a85356e172db8a32aefa983ee1259f60406f69`
- LAST_CLEAN_PRODUCT_TIP (historical):
  `a46eb7e055ef443f5efecdac1cc447a3c1941805`
- Final Windows startup audit tip (audit-only, measurement):
  `be58f53c81188e2f46e122db9bd1c75066b4dff0`

## Rules

- `archive/picoview-20260914/*` tags are HISTORICAL. Do not develop from them.
- Do not develop from `a46eb7e0` merely because it was the old product tip.
- Do not merge, cherry-pick, or submit this branch or the archive tags
  upstream.
- Active development authority is this fork's current product line.
