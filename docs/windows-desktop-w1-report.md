# W1 — PocketJS Windows Desktop Enablement: Final Report

Fork: `jnhu76/pocketjs` · Branch: `feat/windows-desktop`

```text
W1 IMPLEMENTATION:       CODE_COMPLETE_PENDING_VALIDATION
POCKETJS_WINDOWS_DESKTOP: NOT_PROVEN
MARKIT PRODUCT WORK:     UNBLOCKED_TO_START_IN_PARALLEL
```

Code-complete means every W1 mechanism is implemented and green on the
Linux dry-run host. NOT_PROVEN stands until the PLATFORM_INTEGRATION
acceptance run executes on a real Windows machine — that is the hard gate.
These two verdicts coexist on purpose: Markit P0 can start in parallel
(no foundation blocker surfaced), but starting product work is not evidence
that the Windows gate closed.

## 16.1 Baseline

| Item | Value |
| --- | --- |
| Upstream base | `cadffef50b0359e1a069586b9dc5574d65d7fb05` (upstream/main, `docs(blog): derive the embedded agent-native runtime (#281)`) |
| Fork branch | `feat/windows-desktop` |
| Branch HEAD | see `git log --oneline -10` |
| Toolchain | rustc/cargo (Linux x64), Bun 1.3.14, wgpu via llvmpipe (software Vulkan) |
| Dev host | Linux (headless) — no Windows machine attached |
| Old archive | `archive/windows-desktop-wip` @ `1ac09d51` — reference/evidence only; nothing cherry-picked wholesale |

## Commit ledger (this W1 round)

| Commit | Change |
| --- | --- |
| `2c32e80` | `feat(input): primary shortcut modifier — Command on macOS, Control elsewhere` |
| `bc89435` | `feat(clipboard): portable system clipboard with a hardened Win32 backend` |
| `e62c68c` | `feat(cjk): resolve Windows fallback fonts through %WINDIR%/SystemRoot` |
| `c4e7906` | `fix(host): resolve the home directory through USERPROFILE on Windows` |
| `66b2eca` | `feat(host): make the flat host's platform identity configurable` |
| `5bc5674` | `fix(host): drive editing chords off the primary modifier, not super` |
| `b8a8cfc` | `feat(host): word-dance — primary+arrow forwards as WordLeft/WordRight` |
| `f3464a4` | `feat(acceptance): generic text acceptance app for the desktop host` |
| `db80888` | `test(desktop): reproducible acceptance proof run for the generic text surface` |
| `8058657` | `fix(tools): make filesystem URL conversion Windows-safe` |
| `0b41e4b` | `fix(widget): fall back to an opaque surface when alpha is unsupported` |
| `4d877b4` | `feat(host): explicit --form window posture for the flat desktop host` |
| `b9e28f2` | `feat(acceptance): pair the rig on a provisional windows-app target` |
| `2e4e5b8` | `refactor(acceptance): drop rich-text formatting from the gate surface` |

## 16.2 Capability matrix

Evidence levels: `AUTOMATED_PASS` (bun test / headless dry-run), `MANUAL_PASS`, `NOT_TESTED`, `DEFERRED` (mechanism in place, real-Windows evidence pending).

Evidence classes — a headless screenshot and a windowed OS run prove different things and are labeled differently:

- `DESKTOP_WIRE_ACCEPTANCE` (automated, `bun tools/acceptance.ts`): scripted `--type/--paste/--key` inject **svc lines directly**. This proves the svc protocol, the guest editing surface and DrawList rendering. It does NOT exercise the OS input stack (winit → Input → `forward_edits`/`forward_ime` → primary chords), and `--paste` is injected text, not a system clipboard read.
- `PLATFORM_INTEGRATION_ACCEPTANCE` (windowed, on the target OS): the real keyboard/IME/clipboard path through `--form window`. Nothing below claims this on Windows yet.

| Capability | Status | Automated | Manual | Evidence |
| --- | --- | --- | --- | --- |
| Opaque desktop window | fill PROVEN (headless); posture CODE (`--form window`) | opaque fill in `acceptance-proof.png` @2x; posture config: `transparent:false, decorations:true, always_on_top:false` | Windows: DEFERRED | headless render proves the opaque FILL only — no OS window exists headless; posture awaits the windowed run |
| Resize (live re-wrap) | code path in place | headless is fixed-size | Windows: DEFERRED | host `{"t":"resize"}` → `resizeViewport` → `layoutDoc` re-wrap |
| Pointer (click/drag selection) | code path in place | — | Windows: DEFERRED | svc mouse stream → `caretFromX` drag selection |
| Keyboard | DESKTOP_WIRE (svc-injected) | scripted `--type` landed at caret | Windows: DEFERRED | acceptance-proof.png — wire-level, not the OS keyboard path |
| Text input | DESKTOP_WIRE (svc-injected) | typed chars + paste inserted | Windows: DEFERRED | acceptance-proof.png (length + content) — wire-level |
| Primary modifier | PROVEN (unit) | `primary_modifier_is_control_off_macos` + host chord remap | — | `cargo test -p pocket3d --lib input` (5 pass) |
| Clipboard | PROVEN (typecheck Win32 + platform gate test) | `pocket-clipboard` tests on Linux (gate) + `--target x86_64-pc-windows-msvc` check | Windows round-trip: DEFERRED | `cargo check -p pocket-clipboard --target x86_64-pc-windows-msvc` |
| CJK runtime glyphs | PROVEN (mechanism + discovery) | Linux dry-run shows tofu (expected — no Linux CJK face) | Windows: DEFERRED | cjk.rs `%WINDIR%` discovery, msyh.ttc first |
| IME preedit | code path in place | — | Windows: DEFERRED | host `forward_ime` + app preedit underline |
| IME commit | code path in place | — | Windows: DEFERRED | commits arrive as `{"t":"ch"}` |
| Demand rendering | stock mechanism | — | Windows: DEFERRED | governor receipt on windowed exit (note-widget) |
| Clean start/shutdown | PROVEN (Linux headless) | 90-frame run exits cleanly | Windows: DEFERRED | acceptance tool run |

## Path portability audit (bounded)

| Site class | Audited | Changed | Intentionally unchanged |
| --- | --- | --- | --- |
| `tools/*.ts` root/path via `new URL(..).pathname` | 24 files | 24 (→ `tools/fs-url.ts` `fsPath` = `fileURLToPath`) | — |
| `framework/compiler/jsx-plugin.ts` fs paths | 11 consts | 11 | `COMPILER_DIR` (URL-prefix semantics, line 87) |
| `framework/compiler/jsx-plugin.ts` URL objects | lines 104-111 | — | URL-object matching/resolution (`url.pathname` prefix, `new URL(path, url)`) |
| Regression coverage | — | `tests/path-portability.test.ts` (5 tests) | — |

Windows-shaped URLs yield `/C:/…` from `.pathname` and keep `%20`; `fileURLToPath` decodes and strips the drive-letter slash. Tests pin both shapes.

## 16.3 Upstream extraction matrix

| Change | Ownership | Upstream action |
| --- | --- | --- |
| URL/path portability (`tools/fs-url.ts` + 24 sites + tests) | PocketJS | UPSTREAM_NOW — small, tested, no Markit dependency |
| Primary modifier (`Input::primary_down`, host chords, word-dance) | PocketJS | UPSTREAM_NOW — unit-tested, Ctrl/Cmd semantic |
| Portable clipboard (`pocket-clipboard` crate) | PocketJS | UPSTREAM_NOW — hardened Win32 + pbcopy/pbpaste, standalone crate |
| Alpha fallback (opaque when alpha unsupported) | PocketJS | UPSTREAM_NOW — boot resilience, no product dependency |
| Windows CJK font discovery (`%WINDIR%` + candidate list) | PocketJS | UPSTREAM_AFTER_ALIGNMENT — mechanism generic; a `FontProvider` seam is a follow-up, not a W1 blocker |
| `windows-app` desktop target / form / hostAbi | PocketJS architecture | DISCUSSION FIRST — proposal below, no contract file changed |
| Acceptance host identity flag (`--identity/--host-abi`) | PocketJS | UPSTREAM_AFTER_ALIGNMENT — flat host generalization; naming with target discussion |
| Generic acceptance app (`apps/acceptance`) | PocketJS test/example | EVALUATE — demo-shelf admission matrix updated (`acceptance: [false,false,true]`) |
| USERPROFILE fallback (host home dir) | PocketJS | UPSTREAM_NOW (small, with portability commit family) |
| Windows CI | PocketJS | UPSTREAM_AFTER_ALIGNMENT — depends on target contract |

KEEP_DOWNSTREAM (not touched, not planned): Markit LineIndex, Markdown BlockIndex, incremental Markdown parser, Markit view model, benchmark harness, product editor behavior.

## 16.4 Known limitations (honest)

- **Real Windows validation NOT EXECUTED**: no Windows machine on this host. All real-platform cells above are DEFERRED; nothing is claimed PASS on Windows without evidence.
- **IME** not hand-verified anywhere; code path + scripted svc path only. Microsoft Pinyin specifically cannot be simulated in CI.
- **CJK glyph bake** not seen rendering on a real font; Linux host has no CJK candidate (tofu expected and observed).
- **Clipboard Win32** typechecked only (`cargo check --target x86_64-pc-windows-msvc`); round-trip tests are `#[cfg(windows)]`-gated and will run on a Windows machine.
- **`windows-app` is provisional by design**: the formal `POCKET_TARGETS` registry is unchanged; the acceptance rig resolves against a test-local profile (`tools/acceptance-target.ts`) so the bundle and the windowed host pair on one identity (`windows-app`, hostAbi 3). Promoting it to a registered contract — and whether it keeps hostAbi 3 — is the W1-G upstream discussion.
- **Unicode indexing seam (recorded follow-up)**: the Rust IME path converts winit's preedit cursor from byte offset to **Unicode scalar count**, while the guest editor positions its caret in **UTF-16 code units** (JS `string.length`/`slice`). BMP text — ASCII + BMP CJK, the W1 validation charset — is unaffected. Astral codepoints (emoji, CJK Extension B) occupy two JS code units and can land a caret/backspace/IME cursor inside a surrogate pair. W1 validation is scoped to **ASCII + BMP CJK**; unified indexing is a follow-up, not deferred silently.
- **Font candidates** are a fixed reference list (msyh/simsun/simhei/DengXian under %WINDIR%); a registry-based discovery (registry queries / enumeration) is the follow-up seam.
- Linux/macOS desktop implementation is out of scope for this round (mechanisms are platform-neutral by construction).

## 16.5 W1-G — TARGET PROFILE PROPOSAL (PROVISIONAL — NOT YET AN UPSTREAM CONTRACT DECISION)

```text
candidate:  windows-app
platform:   windows
form:       window          (proposed new generic form; distinct from widget)
hostAbi:    TBD by upstream (new number or shared wire generation — discussion)
observed capabilities (what the W1 mechanisms actually deliver):
  display.viewport.live
  input.buttons, input.pointer, input.text, input.ime
  host.clipboard
  text.glyphs.baked, text.glyphs.runtime
```

The acceptance rig already runs against this profile as a test-local
provisional registry (`tools/acceptance-target.ts`: POCKET_TARGETS +
`windows-app`, hostAbi 3) so the bundle and the windowed host pair on one
identity today. Promoting the profile — its id, its hostAbi, its capability
set — into `contracts/spec/platforms.ts` is the upstream decision; until
then the formal registry stays untouched.

Open questions for upstream discussion:

1. **`windows-app` vs a shared desktop abstraction**: `macos-widget` is borderless/always-on-top/widget-form; a decorated resizable window is a different posture. `form=window` should likely become a generic form, with `macos-widget` staying widget-form.
2. **hostAbi**: new number vs reusing the wire generation macos-widget uses (the flat host is the same wire). Registry convention says hosts with the same observable semantics share — but hostAbi is also a wire-generation marker; upstream decides.
3. **Transparent/widget semantics**: separate from normal desktop window. The alpha fallback (opaque) makes transparency an optional capability, not a boot requirement.
4. **Capability truth**: the registry should record what the acceptance rig proved, not product wishes. The acceptance surface is the proof tool.

## 16.6 Verdicts

```text
W1 IMPLEMENTATION: CODE_COMPLETE_PENDING_VALIDATION
  — every mechanism implemented; Linux dry-run green (DESKTOP_WIRE
    acceptance). The windowed --form window host and the paired
    windows-app bundle are ready for the real machine.

POCKETJS_WINDOWS_DESKTOP: NOT_PROVEN
  — PLATFORM_INTEGRATION evidence is the hard gate; no Windows machine
    was available. Validation commands are prepared (below).

UPSTREAM_READINESS: READY_FOR_INCREMENTAL_UPSTREAMING
  — the four UPSTREAM_NOW units (portability, primary modifier,
    clipboard, alpha fallback) are small, independent, tested, and have
    no Markit dependency.

MARKIT: UNBLOCKED_TO_START_IN_PARALLEL
  — no foundation blocker surfaced that blocks Markit product work.
    Markit P0 starting is NOT W1 closing; real-Windows validation of the
    substrate runs alongside, not ahead of, P0.
```

## W1-E/B validation commands (for a Windows machine, in order)

```text
# 1. build the acceptance rig (Windows PowerShell, repo root)
bun install
bun tools/acceptance.ts       # windows-app plan bundle + host + DESKTOP_WIRE
                              #   headless proof -> dist/acceptance-proof.png

# 2. windowed PLATFORM_INTEGRATION acceptance (the NOT_EXECUTED list)
cargo run -p note-widget -- --app acceptance-main --form window ^
  --identity windows-app --host-abi 3 --width 1000 --height 700
  # manual: type, click-drag select, Ctrl+C/V/X, Ctrl+Left/Right word-dance,
  # resize (re-wrap), Microsoft Pinyin preedit + commit,
  # CJK 汉字 visible (msyh.ttc), clean close

# 3. clipboard round-trip (real Windows)
cargo test -p pocket-clipboard --target x86_64-pc-windows-msvc

# 4. CI-capable subset (windows-latest)
bun tools/test.ts
cargo test -p pocket3d -p pocket-clipboard -p pocket-widget

# 5. file evidence
dist/acceptance-proof.png     # opaque surface + typed/pasted text (wire-level)
```
