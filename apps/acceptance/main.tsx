// @title Pocket Acceptance
//
// The generic-text acceptance app: the plain-text editing surface the flat
// host exercises on a real Windows machine. No markdown, no undo stack, no
// menu — just caret, selection, typing, word-dance, copy/paste, basic
// formatting and IME over one plain document. Build for the desktop host:
//
//   bun tools/build.ts acceptance-main --density=2
//   cargo run -p note-widget -- --app acceptance-main --identity windows-app \
//     --host-abi 3 --width 1000 --height 700
//
// Same bundle boots on any ui host; without the widget host's svc channel
// it renders the sample document read-only.

import Acceptance from "./app.tsx";
import { mount } from "@pocketjs/framework";

mount(() => <Acceptance />);