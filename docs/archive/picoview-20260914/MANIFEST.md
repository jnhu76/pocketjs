# MANIFEST — POCKETJS-PICOVIEW-PHASE-A-PRESERVATION-1

- DATE: 2026-09-14
- EXECUTED BY: preservation task POCKETJS-PICOVIEW-PHASE-A-PRESERVATION-1
- TASK ID: POCKETJS-PICOVIEW-PHASE-A-PRESERVATION-1
- GIT IDENTITY USED: git version 2.55.0.windows.5, user fred1 @ host Hu
- REMOTE REACHABILITY CAVEAT: all upstream/fork relationships recorded in this
  manifest are BASED_ON_LOCAL_TRACKING_REFS; REMOTE_FRESHNESS_NOT_ASSUMED at
  audit time. The only network operations performed were the Phase 9 push and
  Phase 10 fetch explicitly authorized by the task.

## AUTHORITATIVE SOURCE

C:\Users\fred1\source\pocketjs (repo1; owns 4 worktrees; origin = upstream only)

## UPSTREAM

https://github.com/pocket-stack/pocketjs (repo1 remote `origin`; main = a5a85356e172db8a32aefa983ee1259f60406f69 at preservation time)

## ARCHIVE REMOTE

https://github.com/jnhu76/pocketjs (added to repo1 as remote `fork` by this task; pre-existing personal fork, proven by the independent clone C:\Users\fred1\source\jnhu_pocketjs whose origin is the same fork)

## ORIGINAL LOCAL-ONLY BRANCHES → ARCHIVE TAGS

All tips verified by `git branch -vv` / `git show-ref`; none had an upstream
(BASED_ON_LOCAL_TRACKING_REFS) except the baseline alias.

| original branch (repo1) | tip commit | archive tag |
|---|---|---|
| picoview-a1-windows-target | 62ee522ff79b68730450844775748fe0a3bffc24 | archive/picoview-20260914/picoview-a1-windows-target |
| picoview-a2-image-resource | fe32ea825e05246d8a8dd7d28cb122cedd42ee92 | archive/picoview-20260914/picoview-a2-image-resource |
| picoview-a3-wic-first-jpeg | fa9361297de3ed9fe66f84cfb25c053b2584892a | archive/picoview-20260914/picoview-a3-wic-first-jpeg |
| picoview-a4-large-jpeg-fit | 7979c1d8fde1969dbb25ddb103bdd0c9919480e1 | archive/picoview-20260914/picoview-a4-large-jpeg-fit |
| picoview-a5-cancel-bounds | a832dc8ee99073896d9e3b9dc12db3c765438e78 | archive/picoview-20260914/picoview-a5-cancel-bounds |
| picoview-a6-dpi | af383e04416a1125bee0b4e7e979f67bd15a38b3 | archive/picoview-20260914/picoview-a6-dpi |
| picoview-a7-footprint | 6efb25b74b9d5774294b88d1da1c37762d357952 | archive/picoview-20260914/picoview-a7-footprint |
| picoview-c1-startup-init | d1ae693efe26193b51712dbf8c375e352d3a7b6f | archive/picoview-20260914/picoview-c1-startup-init |
| picoview-c2-minimal-host | 8e9e09f98347c7791c3a32db853367f1db0c1a1d | archive/picoview-20260914/picoview-c2-minimal-host |
| picoview-c3-idle-suspend | 2d35706f244615fad3184baa3f20ebb00fdf7025 | archive/picoview-20260914/picoview-c3-idle-suspend |
| picoview-c4-present-pacing | e15674db1ca9179732c22e5a9a191ba21ebf3e70 | archive/picoview-20260914/picoview-c4-present-pacing |
| audit/windows-startup-callpath-1 | 9a1b8d690858462ce7afc9b8065622cc960584b0 | archive/picoview-20260914/audit-windows-startup-callpath-1 |
| audit/windows-startup-etw-1 | be58f53c81188e2f46e122db9bd1c75066b4dff0 | archive/picoview-20260914/audit-windows-startup-etw-1 |
| scratch/dx12-experiment | 6efb25b74b9d5774294b88d1da1c37762d357952 | archive/picoview-20260914/scratch-dx12-experiment |
| pico-arch-a (baseline alias, = main) | a5a85356e172db8a32aefa983ee1259f60406f69 | archive/picoview-20260914/pico-arch-a |
| (product boundary marker) | a46eb7e055ef443f5efecdac1cc447a3c1941805 | archive/picoview-20260914/last-clean-product-tip |
| (fsck-recovered dropped stash, c2 WIP) | a2251d950b6d3772cc49190a6310e67b442ccee8 | archive/picoview-20260914/recovered/a2251d95 |
| (fsck-recovered dropped stash, a4-wip) | 6c2869e62a2eae3dbc5f916b383f9c79e1a4900c | archive/picoview-20260914/recovered/6c2869e6 |
| (fsck-recovered superseded a3 draft) | 04004fe3a26c5b8ff15b13c29704446c2fcff83f | archive/picoview-20260914/recovered/04004fe3 |
| (fsck-recovered superseded a3 draft) | 3165e5de7dcb86e169fe7534211b5806f99029eb | archive/picoview-20260914/recovered/3165e5de |
| (fsck-recovered superseded a3 draft) | 7ac9da2e543cac6170a0be0d4d5a00071e47c12a | archive/picoview-20260914/recovered/7ac9da2e |

21 annotated tags total. Tag-object SHAs are recorded in REFS.txt and were
confirmed identical on the fork via `git ls-remote` (42 refs = 21 tags x {tag
object, peeled commit}).

## WORKTREE MAP (all unchanged by this task)

| path | branch | HEAD |
|---|---|---|
| C:\Users\fred1\source\pocketjs | picoview-c4-present-pacing | e15674db1ca9179732c22e5a9a191ba21ebf3e70 |
| C:\Users\fred1\source\pocketjs-c2base | (detached) | 8e9e09f98347c7791c3a32db853367f1db0c1a1d |
| C:\Users\fred1\source\pocketjs-win-etw | audit/windows-startup-etw-1 | be58f53c81188e2f46e122db9bd1c75066b4dff0 |
| C:\Users\fred1\source\pocketjs-win-startup | audit/windows-startup-callpath-1 | 9a1b8d690858462ce7afc9b8065622cc960584b0 |

Independent fork workspace (NOT the source of truth; not modified except by
the Phase 10 tag fetch):
C:\Users\fred1\source\jnhu_pocketjs — origin = jnhu76/pocketjs (ssh),
upstream = pocket-stack/pocketjs; branch feat/windows-desktop-parity @
4082abcb3a0302553be8f87241c787e1c196fc16, in sync (+0 -0), clean.

## PRODUCT HISTORY (PicoView Phase A chain)

Base: a5a85356e172db8a32aefa983ee1259f60406f69 (upstream main, frozen PocketJS
campaign baseline, also recorded in PicoView docs/POCKETJS-BASELINE.md)

Ordered chain (all PRODUCT unless annotated):

1. 62ee522ff79b68730450844775748fe0a3bffc24 feat(desktop): extend portable host to Windows with windows-app stock target (A1)
2. fe32ea825e05246d8a8dd7d28cb122cedd42ee92 feat(core,desktop): native image-resource seam for large guest-owned composition (A2)
3. fa9361297de3ed9fe66f84cfb25c053b2584892a feat(desktop): WIC-first JPEG proof harness feeding the native image seam (A3)
4. 7979c1d8fde1969dbb25ddb103bdd0c9919480e1 feat(desktop): WIC source-transform scaled decode for viewport-fit JPEG (A4)
5. a832dc8ee99073896d9e3b9dc12db3c765438e78 feat(desktop): coalescing drain - cancel superseded opens before decode (A5)
6. 09842209263fed7bc7dab4f0bd1af2a91ea78b48 feat(desktop): Per-Monitor DPI V2 transitions on the viewer path (A6)
7. af383e04416a1125bee0b4e7e979f67bd15a38b3 test: correct effective_density label (A6 follow-up)
8. a4806320184a8649fd17fbbba28f5fec6c808e8a feat(desktop): monotonic event clock, IMGREADY marker, startup phases (A7)
9. 6efb25b74b9d5774294b88d1da1c37762d357952 fix(desktop): trace_frame joins the shared monotonic clock (A7)
10. 34d371d1360005af7d2e003fb857a4620e071fca feat(desktop): minimal product host + staged residency probe (C2)
11. 8e9e09f98347c7791c3a32db853367f1db0c1a1d fix(desktop): C2 review corrections
12. 23f634cd5d1d5ce34c2d1c8f52b44cf8256a74d9 perf(desktop): overlap GPU/ICD init with window+guest startup (C1)
13. d1ae693efe26193b51712dbf8c375e352d3a7b6f fix(desktop): gate the A6 dpi-awareness line behind the measurement flag (C1 review)
14. d8ef6b6671d6effbceed43b46ec089d75ee34430 feat(runtime,desktop): event-driven idle suspend for the runtime worker (C3)
15. 2d35706f244615fad3184baa3f20ebb00fdf7025 fix(runtime,desktop): C3 review corrections
16. a46eb7e055ef443f5efecdac1cc447a3c1941805 feat(desktop): C4 input->present decomposition traces + parked trace mode (C4)

LAST_CLEAN_PRODUCT_TIP: a46eb7e055ef443f5efecdac1cc447a3c1941805
(verified by commit graph + diff inspection: the only commits after it on the
branch are the two MEASUREMENT_ONLY commits below)

## MEASUREMENT / AUDIT HISTORY

MEASUREMENT_ONLY, on picoview-c4-present-pacing after the product tip:
17. 50e3ed8e8b0a196c0710d39f78458b02d978878a meas(desktop): CROSS-OS-NORMALIZED-DESKTOP-STARTUP-1 instrumentation
18. e15674db1ca9179732c22e5a9a191ba21ebf3e70 meas(desktop): BENCHMARK_CONFIG to stderr (one evidence stream per run)

AUDIT_ONLY + DEPENDENCY_INSTRUMENTATION, both audit branches fork at
e15674db and contain, in order:
19. 2ca5dc4c30424468b02a1b5f0e0efff45d4772d0 meas(desktop): arm B follows POCKET_GPU_BACKEND (Windows audit default DX12)
20. 66df576f413e1545bc877dd5e32dac8531e6f009 meas(desktop): vendored wgpu-hal dx12 nested markers E50a-h (audit-only)
21. 4b2de0a47d7aa755d38dac192020de951ae26ebf meas(wgpu-hal): name+LUID per exposed DXGI adapter (audit-only)
22. 9a1b8d690858462ce7afc9b8065622cc960584b0 meas(desktop): lockfile for vendored wgpu-hal patch (audit-only)  ← callpath-1 tip
23. be58f53c81188e2f46e122db9bd1c75066b4dff0 meas(desktop): WINDOWS-STARTUP-ETW-REALITY-AUDIT-1 exact-LUID skip + selected-LUID carry (audit-only)  ← FINAL AUDIT TIP (superset of callpath-1)

## PICOVIEW RELATION

- Consolidated experimental record: PicoView Draft PR #44
  (DESKTOP-STARTUP-PERFORMANCE-EXPERIMENTAL-RECORD-1), branch
  docs/startup-performance-experimental-record-1, base ee04801.
  PR #44 preserves: selected-evidence archives, patch series including
  wgpu-hal-exact-luid-skip.patch and wgpu-hal-marker-instrumentation.patch,
  source-identity manifests, evidence index. It does NOT preserve commit
  identity/ancestry — the bundle and archive tags in this preservation do.
- Earlier PicoView tickets consumed this chain via pinned sibling SHAs
  (A1–A7 → PR #20–#24, GATE-A → PR #30, C1–C4 → PR #33–#35; GATE-A2 and the
  startup campaigns consumed a46eb7e0 and the measurement layer).

## KNOWN AUDIT-ONLY CONTENT (do not mistake for product)

- nested dx12 markers E50a-h inside vendored wgpu-hal (dependency instrumentation)
- vendored wgpu-hal patch + its lockfile entry
- DXGI adapter name+LUID emission
- POCKET_GPU_BACKEND arm selector (audit default DX12)
- exact-LUID skip + selected-LUID carry (audit-only by explicit authority)

## DO NOT USE AS PRODUCT BASE

- e15674db / 50e3ed8e (measurement layer on c4)
- 9a1b8d69, be58f53c, 2ca5dc4c, 66df576f, 4b2de0a4 (audit-only series)
- recovered/* objects (dropped stashes, superseded drafts)
- any archive/picoview-20260914/* tag as a development starting point

## RECOMMENDED FUTURE PRODUCT BASE

a46eb7e055ef443f5efecdac1cc447a3c1941805 (tag archive/picoview-20260914/last-clean-product-tip).
Why: newest commit whose ancestry contains only accepted product work; it is
the exact PocketJS identity PicoView's C4/GATE-A2 evidence was recorded
against. If upstream main advances, re-base product work on upstream and treat
a46eb7e0 as the PicoView-specific delta reference. Future capabilities that
exist only inside the audit series (e.g., GPU backend selection) are
REIMPLEMENT_LATER_IF_NEEDED as clean independent changes, not cherry-picks.

## UNTRACKED

See UNTRACKED.txt. Summary: evidence/ (172 files, 2.8 MB, UNIQUE_MUST_ARCHIVE,
archived locally, LOCAL-ARCHIVE-ONLY) ; guest/ empty ; ~5.4 GB target-*/ build
outputs REBUILDABLE_DO_NOT_ARCHIVE.

## BUNDLE

- file: pocketjs-picoview-20260914.bundle
- created from: repo1, `git bundle create --all HEAD`
- size: 104807739 bytes (~100 MB)
- sha256: 898be752c37e9b09ea5fdde6e76d7bc5c5473df0b13e65dead9143727a7fd890
- `git bundle verify`: PASS ("The bundle records a complete history.")
- recovery test: clone from bundle into a temporary directory → all 21 archive
  tags present; every tag resolved to the exact expected commit SHA; tree SHAs
  identical to source for be58f53c (7a9c61447111e9537b8ffd7ce5db74eb4f6dbf27),
  a46eb7e0 (d12b28b7f0246c5ae1a032af55182113ec2ce86d), e15674db
  (f09cf9fa116d3fa92d64f49fec7edf1d088d059e); origin/main = a5a85356 present;
  measurement commits 50e3ed8e/2ca5dc4c/66df576f/4b2de0a4 present. Temp clone
  deleted after PASS. Note: the bundle does NOT contain the two untagged
  garbage blobs recorded in FSCK.txt (they are unreachable by definition and
  were judged worthless; the source repo still holds them untouched).

## UNIQUE-EVIDENCE ARCHIVE (separate from the bundle)

- file: pocketjs-untracked-unique-evidence-20260914.tar.gz (180 tar entries =
  172 log files + 8 directories)
- sha256: a438a96d74225c0d26ca263b780b5db446e4b9d2188d398a492f0b37f3f6a69c
- LOCAL-ARCHIVE-ONLY: logs contain local machine paths/username; do not push
  or redistribute.

## REMOTE ARCHIVE (jnhu76/pocketjs)

- push: only refs/tags/archive/picoview-20260914/* (42 refs = 21 tags), via
  repo1 remote `fork` (https). No branch was pushed; main, master,
  feat/windows-desktop-parity and all other fork branches untouched.
- remote verification: `git ls-remote` confirms all 42 refs; peeled commit
  SHAs match the table above exactly (sampled exhaustively for audit/etw/
  product-tip/baseline and spot-checked across a1–c2; full 21/21 confirmed
  again in Phase 10).
- Phase 10 independent recovery: C:\Users\fred1\source\jnhu_pocketjs fetched
  the same tag namespace over ssh → 21/21 tags present; verified exact SHAs:
  be58f53c (etw final), a46eb7e0 (product tip), e15674db (c4 meas tip),
  a2251d95 (recovered stash); tree of be58f53c = 7a9c6144... identical to
  source. Chain of custody proven: local repo1 → fork → independent clone.

## SECRET-SAFETY

Committed-path scan of `git log --all --name-only`: two benign hits
(tls_smoke fixture ca.cert.pem = public CA cert fixture; site/assets/tokens.css
= design tokens). Untracked evidence inputs: filename + content pattern scan
clean (no private keys, tokens, credentials). Logs are local-identifying only.
Result: SAFE_TO_ARCHIVE for the local archive set; no history rewrite
performed; the bundle therefore carries the full upstream + local history as
committed.

## WORKTREES

Unchanged. No branch was merged, rebased, reset, deleted, or checked out; no
untracked file was cleaned; PicoView was not modified; no performance test was
run.

## FILES IN THIS DIRECTORY

- pocketjs-picoview-20260914.bundle
- pocketjs-untracked-unique-evidence-20260914.tar.gz
- MANIFEST.md (this file)
- REFS.txt (git show-ref snapshot incl. all archive tags + tag-object SHAs)
- WORKTREES.txt (git worktree list --porcelain snapshot)
- UNTRACKED.txt (untracked/ignored inventory + classifications)
- FSCK.txt (dangling-object audit record: 7 recovered commits, 2 garbage blobs)
- SHA256SUMS (hashes of every file in this directory)
