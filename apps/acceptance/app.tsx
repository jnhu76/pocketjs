// apps/acceptance/app.tsx — Pocket Acceptance: the generic text editor.
//
// Deliberately the smallest thing that exercises every desktop-host
// mechanism the acceptance rig exists to prove, over ONE plain-string
// document: live typing with a caret, drag selection, word-dance
// (Primary+Left/Right), copy/cut/paste through the system clipboard, wheel
// scroll, live resize re-wrap, IME composition and runtime CJK glyphs.
// There is no markdown, no rich text, no undo/redo and no save — the host
// only forwards input and mirrors the clipboard. Rich-text formatting is
// out of scope on purpose: W1 proves the substrate, not an editor product.
//
// The editing math is the same framework-free editor.ts the note app uses
// (soft wrap + caret/selection over a string); this file is the thin
// Solid surface around it. The svc channel (svc.ts) is the flat host's
// wire protocol: keys/chars/IME/mouse/scroll/resize in, copy/caret out.

import { createMemo, createSignal, For, Show } from "solid-js";
import { Focusable, Text, View } from "@pocketjs/framework/components";
import { onFrame } from "@pocketjs/framework/lifecycle";
import { focusNode, hitFocusable } from "@pocketjs/framework/input";
import { getOps, resizeViewport } from "@pocketjs/framework";
import { hasFeature } from "@pocketjs/framework/platform";
import {
  backspaceSel,
  caretFromX,
  caretLine,
  caretX,
  deleteSel,
  layoutDoc,
  lineEnd,
  lineStart,
  moveVertical,
  selBounds,
  typeText,
  type SelEdit,
} from "../note/editor.ts";
import { connectSvc, type HostEvent } from "../note/svc.ts";

const HEADER_H = 30;
const EDGE_PAD = 10;
const BODY_LINE_H = 20;
const CARET_H = 16;
const SCROLL_STEP = 40;
const OVERSCAN = 40;
const DRAG_SLOP = 3;

const INK = {
  body: "#d7dee7",
  dim: "#7e8994",
  accent: "#6fb3ff",
  sel: "#2a4a6e",
  header: "#8d98a5",
  thumb: "#414d5b",
};

/** Build-pinned font slot for body text (layout.ts FONT_BODY). */
const FONT_BODY = 1;

/** The editor's measure fn (body font) — same contract as editor.ts. */
function measure(text: string): number {
  return getOps().measureText(text, FONT_BODY);
}

/** A char counts as a word character: ASCII word, or any non-ASCII glyph
 *  (CJK moves one hanzi at a time — the natural word unit). */
function isWordChar(c: string): boolean {
  const code = c.codePointAt(0)!;
  return (
    (code >= 0x30 && code <= 0x39) ||
    (code >= 0x41 && code <= 0x5a) ||
    (code >= 0x61 && code <= 0x7a) ||
    code > 0x7f
  );
}

/** Caret → start of the current/previous word (word-dance: Primary+Left). */
function wordLeft(doc: string, pos: number): number {
  let i = pos;
  if (i > 0 && isWordChar(doc[i - 1])) {
    while (i > 0 && isWordChar(doc[i - 1])) i--;
    return i;
  }
  while (i > 0 && !isWordChar(doc[i - 1])) i--;
  while (i > 0 && isWordChar(doc[i - 1])) i--;
  return i;
}

/** Caret → end of the current/next word (word-dance: Primary+Right). */
function wordRight(doc: string, pos: number): number {
  let i = pos;
  while (i < doc.length && isWordChar(doc[i])) i++;
  while (i < doc.length && !isWordChar(doc[i])) i++;
  return i;
}

const SAMPLE_DOC =
  "Pocket Acceptance — a plain text surface.\n\n" +
  "Type here. Select with the mouse. Copy (Primary+C), cut (Primary+X),\n" +
  "paste (Primary+V). Word-dance with Primary+Left/Right. IME composition\n" +
  "and CJK 汉字 render through the runtime glyph atlas. Resize the window —\n" +
  "the text re-wraps. Scroll with the wheel. Every row is the stock host\n" +
  "mechanism doing its real job.";

export default function Acceptance(): ReturnType<typeof View> {
  const svc = connectSvc();
  const canEdit = hasFeature("input.text");
  const [vp, setVp] = createSignal({ w: 480, h: 272 });
  const [doc, setDoc] = createSignal(SAMPLE_DOC);
  const [caret, setCaret] = createSignal(0);
  const [anchor, setAnchor] = createSignal(0);
  const [preedit, setPreedit] = createSignal<{ text: string; cursor: number } | null>(null);
  const [scrollE, setScrollE] = createSignal(0);

  const viewH = () => vp().h - HEADER_H;
  const contentW = () => Math.max(40, vp().w - EDGE_PAD * 2);

  // ---- edit layout -------------------------------------------------------
  const displayDoc = createMemo(() => {
    const p = preedit();
    if (!p) return doc();
    return doc().slice(0, caret()) + p.text + doc().slice(caret());
  });
  const displayCaret = () => {
    const p = preedit();
    return p ? caret() + p.cursor : caret();
  };
  const dlines = createMemo(() => layoutDoc(displayDoc(), contentW(), measure));
  const editTotal = () => dlines().length * BODY_LINE_H + EDGE_PAD * 2;
  const maxScrollE = () => Math.max(0, editTotal() - viewH());
  const visibleLines = createMemo(() => {
    const lines = dlines();
    const from = Math.max(0, Math.floor((scrollE() - OVERSCAN - EDGE_PAD) / BODY_LINE_H));
    const to = Math.min(
      lines.length,
      Math.ceil((scrollE() + viewH() + OVERSCAN - EDGE_PAD) / BODY_LINE_H),
    );
    const out: { index: number; start: number; end: number; soft: boolean }[] = [];
    for (let i = from; i < to; i++) {
      out.push({ index: i, start: lines[i].start, end: lines[i].end, soft: lines[i].soft });
    }
    return out;
  });
  const caretRow = () => caretLine(dlines(), displayCaret());
  const caretPx = () => caretX(displayDoc(), dlines(), displayCaret(), measure);
  const editSel = () => {
    if (caret() === anchor()) return null;
    const [lo, hi] = selBounds({ doc: doc(), caret: caret(), anchor: anchor() });
    return { lo, hi };
  };

  // ---- edits --------------------------------------------------------------
  let goalX = 0;
  let goalSticky = false;
  let lastHover: unknown = null;
  let prevDown = false;
  let press: { x: number; y: number; dragged: boolean } | null = null;

  const revealCaret = () => {
    const y = EDGE_PAD + caretRow() * BODY_LINE_H;
    if (y < scrollE() + 4) setScrollE(Math.max(0, y - 4));
    else if (y + BODY_LINE_H > scrollE() + viewH() - 4) {
      setScrollE(Math.min(maxScrollE(), y + BODY_LINE_H - viewH() + 4));
    }
  };
  const selState = (): SelEdit => ({ doc: doc(), caret: caret(), anchor: anchor() });
  const applyState = (s: SelEdit) => {
    setDoc(s.doc);
    setCaret(s.caret);
    setAnchor(s.anchor);
  };
  const mutate = (fn: (s: SelEdit) => SelEdit) => {
    applyState(fn(selState()));
    goalSticky = false;
    revealCaret();
  };
  /** Collapse-or-move for plain arrows (a selection collapses to its edge). */
  const collapseOr = (edge: "lo" | "hi", move: (s: SelEdit) => number) => {
    const s = selState();
    const [lo, hi] = selBounds(s);
    const pos = s.caret === s.anchor ? move(s) : edge === "lo" ? lo : hi;
    setCaret(pos);
    setAnchor(pos);
    goalSticky = false;
  };

  const handleKey = (k: string, shift = false) => {
    const extend = (pos: number) => {
      setCaret(Math.max(0, Math.min(pos, doc().length)));
      revealCaret();
    };
    if (shift && !preedit()) {
      switch (k) {
        case "Left":
          extend(caret() - 1);
          return;
        case "Right":
          extend(caret() + 1);
          return;
        case "WordLeft":
          extend(wordLeft(doc(), caret()));
          return;
        case "WordRight":
          extend(wordRight(doc(), caret()));
          return;
        case "Home":
          extend(lineStart(dlines(), caret()));
          return;
        case "End":
          extend(lineEnd(dlines(), caret()));
          return;
        case "Up":
        case "Down": {
          if (!goalSticky) {
            goalX = caretPx();
            goalSticky = true;
          }
          extend(moveVertical(doc(), dlines(), caret(), k === "Up" ? -1 : 1, goalX, measure));
          return;
        }
      }
    }
    if (k === "Escape") {
      if (editSel()) setAnchor(caret());
      return;
    }
    if (k === "Copy") {
      const sel = editSel();
      if (sel) svc?.send({ t: "copy", text: doc().slice(sel.lo, sel.hi) });
      return;
    }
    if (k === "Cut") {
      const sel = editSel();
      if (sel) {
        svc?.send({ t: "copy", text: doc().slice(sel.lo, sel.hi) });
        mutate((s) => typeText(s, ""));
      }
      return;
    }
    switch (k) {
      case "Backspace":
        mutate(backspaceSel);
        break;
      case "Delete":
        mutate(deleteSel);
        break;
      case "Enter":
        mutate((s) => typeText(s, "\n"));
        break;
      case "Tab":
        mutate((s) => typeText(s, "  "));
        break;
      case "Left":
        collapseOr("lo", (s) => Math.max(0, s.caret - 1));
        break;
      case "Right":
        collapseOr("hi", (s) => Math.min(s.doc.length, s.caret + 1));
        break;
      case "WordLeft":
        collapseOr("lo", (s) => wordLeft(s.doc, s.caret));
        break;
      case "WordRight":
        collapseOr("hi", (s) => wordRight(s.doc, s.caret));
        break;
      case "Home":
        collapseOr("lo", (s) => lineStart(dlines(), s.caret));
        break;
      case "End":
        collapseOr("hi", (s) => lineEnd(dlines(), s.caret));
        break;
      case "Up":
      case "Down": {
        if (!goalSticky) {
          goalX = caretPx();
          goalSticky = true;
        }
        const next = moveVertical(doc(), dlines(), caret(), k === "Up" ? -1 : 1, goalX, measure);
        setCaret(Math.max(0, Math.min(next, doc().length)));
        setAnchor(caret());
        revealCaret();
        break;
      }
      case "PageUp":
        setCaret(0);
        setAnchor(0);
        revealCaret();
        break;
      case "PageDown":
        setCaret(doc().length);
        setAnchor(doc().length);
        revealCaret();
        break;
    }
  };

  // ---- pointer gestures (svc mouse stream) --------------------------------
  const editPosAt = (x: number, y: number): number => {
    const line = Math.floor((y - HEADER_H + scrollE() - EDGE_PAD) / BODY_LINE_H);
    return caretFromX(doc(), dlines(), line, x - EDGE_PAD, measure);
  };

  const pointerDown = (x: number, y: number, shift: boolean) => {
    press = { x, y, dragged: false };
    if (y < HEADER_H || preedit()) return;
    const pos = editPosAt(x, y);
    setCaret(pos);
    if (!shift) setAnchor(pos);
    goalSticky = false;
  };
  const pointerMove = (x: number, y: number, down: boolean) => {
    if (!down || !press) return;
    if (!press.dragged && Math.abs(x - press.x) + Math.abs(y - press.y) < DRAG_SLOP) return;
    press.dragged = true;
    setCaret(editPosAt(x, y));
    revealCaret();
  };
  const pointerUp = () => {
    press = null;
  };

  const handleEvent = (ev: HostEvent) => {
    switch (ev.t) {
      case "hello":
      case "resize":
        setVp({ w: ev.w ?? 480, h: ev.h ?? 272 });
        resizeViewport(ev.w ?? 480, ev.h ?? 272);
        setScrollE(Math.max(0, Math.min(maxScrollE(), scrollE())));
        break;
      case "ch":
        if (canEdit && ev.s) {
          setPreedit(null); // a commit replaces the preedit it finalizes
          mutate((s) => typeText(s, ev.s!));
        }
        break;
      case "paste":
        if (canEdit && ev.text) mutate((s) => typeText(s, ev.text!));
        break;
      case "ime": {
        if (!canEdit) break;
        const text = ev.s ?? "";
        if (text === "") {
          setPreedit(null);
          break;
        }
        if (editSel()) mutate((s) => typeText(s, ""));
        setPreedit({ text, cursor: Math.min(ev.c ?? text.length, text.length) });
        revealCaret();
        break;
      }
      case "key":
        if (ev.k) handleKey(ev.k, ev.sh ?? false);
        break;
      case "mouse": {
        const p = { x: ev.x ?? -1, y: ev.y ?? -1 };
        const down = ev.d ?? false;
        if (down && !prevDown) pointerDown(p.x, p.y, ev.sh ?? false);
        else if (down) pointerMove(p.x, p.y, true);
        if (!down && prevDown) pointerUp();
        prevDown = down;
        const n = hitFocusable(p.x, p.y);
        if (n && n !== lastHover) focusNode(n);
        lastHover = n;
        break;
      }
      case "scroll": {
        const dy = ev.dy ?? 0;
        setScrollE(Math.max(0, Math.min(maxScrollE(), scrollE() - dy)));
        break;
      }
    }
  };

  let lastCaretRect = { x: -1, y: -1, h: -1 };
  onFrame(() => {
    if (!svc) return;
    for (const ev of svc.poll()) handleEvent(ev);
    const rect = {
      x: Math.round(EDGE_PAD + caretPx()),
      y: Math.round(HEADER_H + EDGE_PAD + caretRow() * BODY_LINE_H - scrollE()),
      h: BODY_LINE_H,
    };
    if (rect.x !== lastCaretRect.x || rect.y !== lastCaretRect.y) {
      lastCaretRect = rect;
      svc.send({ t: "caret", ...rect });
    }
  });

  // ---- render ------------------------------------------------------------
  const lineSelRect = (line: { index: number; start: number; end: number }) => {
    const sel = editSel();
    if (!sel) return null;
    const lo = Math.max(sel.lo, line.start);
    const hi = Math.min(sel.hi, line.end);
    if (hi < lo) return null;
    if (hi === lo && !(sel.lo < line.start && sel.hi > line.end)) return null;
    return {
      x0: measure(displayDoc().slice(line.start, lo)),
      x1: measure(displayDoc().slice(line.start, hi)),
    };
  };
  const preeditRect = (line: { index: number; start: number; end: number }) => {
    const p = preedit();
    if (!p) return null;
    const lo = Math.max(caret(), line.start);
    const hi = Math.min(caret() + p.text.length, line.end);
    if (hi <= lo) return null;
    return {
      x0: measure(displayDoc().slice(line.start, lo)),
      x1: measure(displayDoc().slice(line.start, hi)),
    };
  };
  const thumbH = (total: number) => Math.max(24, (viewH() * viewH()) / total);
  const scrollbar = (scroll: number, total: number) => (
    <Show when={total > viewH()}>
      <View
        class="absolute rounded-sm"
        style={{
          width: 3,
          insetR: 3,
          insetT: (scroll / (total - viewH())) * (viewH() - thumbH(total) - 8) + 4,
          height: thumbH(total),
          bgColor: INK.thumb,
        }}
      />
    </Show>
  );

  return (
    <View class="flex-col w-full h-full bg-[#11151b]">
      <View class="flex-row items-center gap-2 px-3" style={{ height: HEADER_H }}>
        <Text class="text-xs font-bold tracking-wide" style={{ textColor: INK.header }}>
          POCKET ACCEPTANCE
        </Text>
        <View class="flex-1" />
        <Text class="text-xs" style={{ textColor: INK.dim }}>
          {doc().length}
        </Text>
      </View>

      <Focusable class="relative flex-1 overflow-hidden">
        <View
          class="absolute"
          style={{
            insetL: EDGE_PAD,
            width: contentW(),
            insetT: 0,
            height: editTotal(),
            translateY: -scrollE(),
          }}
        >
          <For each={visibleLines()}>
            {(line) => (
              <View
                class="absolute left-0 right-0"
                style={{ insetT: EDGE_PAD + line.index * BODY_LINE_H, height: BODY_LINE_H }}
              >
                <Show when={lineSelRect(line) != null}>
                  <View
                    class="absolute rounded-sm"
                    style={{
                      insetL: lineSelRect(line)?.x0 ?? 0,
                      insetT: 1,
                      width: Math.max(2, (lineSelRect(line)?.x1 ?? 0) - (lineSelRect(line)?.x0 ?? 0)),
                      height: BODY_LINE_H - 2,
                      bgColor: INK.sel,
                    }}
                  />
                </Show>
                <Show when={preeditRect(line) != null}>
                  <View
                    class="absolute rounded-sm"
                    style={{
                      insetL: preeditRect(line)?.x0 ?? 0,
                      insetT: BODY_LINE_H - 3,
                      width: Math.max(2, (preeditRect(line)?.x1 ?? 0) - (preeditRect(line)?.x0 ?? 0)),
                      height: 2,
                      bgColor: INK.accent,
                    }}
                  />
                </Show>
                <Text
                  class="absolute text-sm"
                  style={{
                    insetL: 0,
                    insetT: 0,
                    height: BODY_LINE_H,
                    lineHeight: BODY_LINE_H,
                    textColor: INK.body,
                  }}
                >
                  {displayDoc().slice(line.start, line.end)}
                </Text>
              </View>
            )}
          </For>
          <Show when={!editSel() && !preedit()}>
            <View
              class="absolute animate-pulse rounded-sm"
              style={{
                width: 2,
                insetL: caretPx() - 1,
                insetT: EDGE_PAD + caretRow() * BODY_LINE_H + (BODY_LINE_H - CARET_H) / 2,
                height: CARET_H,
                bgColor: INK.accent,
              }}
            />
          </Show>
        </View>
        {scrollbar(scrollE(), editTotal())}
      </Focusable>
    </View>
  );
}