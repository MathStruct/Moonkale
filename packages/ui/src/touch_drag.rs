//! Dragging tabs by touch (Prompt26: "Android: one can not drag and drop to
//! rearrange windows").
//!
//! The workbench docks a tab by HTML5 drag and drop (`draggable`,
//! `dragstart`, `dragenter`/`dragover` on its drop zones, `drop`,
//! `dragend`), and a finger never starts an HTML5 drag in the Android
//! WebView. This document-level script turns a **long press** on a tab into
//! that same sequence of events, dispatched as synthetic `DragEvent`s at
//! whatever is under the finger, so the workbench's own handlers do the
//! docking — no fork of `dioxus-workbench`. A finger that moves before the
//! press is long enough scrolls the tab strip as before; a short tap is a
//! click. Mouse and pen are untouched (touch events only), so it is
//! installed everywhere.
//!
//! While a finger holds a tab, the tab is not `draggable`: the Android
//! WebView starts its *own* native drag of a draggable element on a long
//! press, which cancels the touch sequence (`touchcancel`) halfway and drops
//! wherever the finger is, onto no dock zone. Found on the Galaxy S10e;
//! Chromium's touch emulation on the desktop does not do that.

use dioxus::prelude::*;

/// Install once per document (idempotent).
pub(crate) fn install() {
    dioxus::core::spawn_forever(async move {
        let _ = document::eval(SCRIPT).await;
    });
}

const SCRIPT: &str = r#"
if (window.__mkTouchDrag) return;
window.__mkTouchDrag = true;
const HOLD_MS = 350;   // a press this long on a tab starts a drag
const SLOP_PX = 10;    // moving further before that is a scroll
let press = null;      // { tab, x, y, timer }
let drag = null;       // { tab, dt, over, x, y }
const fire = (el, type, x, y, dt) => {
  if (!el) return;
  el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, dataTransfer: dt }));
};
// A held tab is not draggable (see above); every way a press ends restores it.
let held = null;
const hold = (tab) => { release(); if (tab.draggable) { tab.draggable = false; held = tab; } };
const release = () => { if (held) { held.draggable = true; held = null; } };
const cancelPress = () => { if (press) { clearTimeout(press.timer); press = null; } if (!drag) release(); };
const begin = () => {
  const p = press; press = null;
  if (!p || !p.tab.isConnected) return;
  let dt = null;
  try { dt = new DataTransfer(); dt.setData("text/plain", p.tab.id || "wb-tab"); dt.effectAllowed = "move"; } catch (_) {}
  drag = { tab: p.tab, dt, over: null, x: p.x, y: p.y };
  document.documentElement.classList.add("mk-touch-dragging");
  try { navigator.vibrate && navigator.vibrate(12); } catch (_) {}
  fire(p.tab, "dragstart", p.x, p.y, dt);
};
const finish = (dropIt) => {
  const d = drag; drag = null;
  release();
  document.documentElement.classList.remove("mk-touch-dragging");
  if (!d) return;
  if (dropIt) {
    const el = document.elementFromPoint(d.x, d.y);
    fire(el, "dragover", d.x, d.y, d.dt);
    fire(el, "drop", d.x, d.y, d.dt);
  } else if (d.over) {
    fire(d.over, "dragleave", d.x, d.y, d.dt);
  }
  fire(d.tab.isConnected ? d.tab : document.querySelector(".wb-workspace"), "dragend", d.x, d.y, d.dt);
};
document.addEventListener("touchstart", (e) => {
  if (e.touches.length !== 1) { cancelPress(); return; }
  const tab = e.target && e.target.closest ? e.target.closest(".wb-tab") : null;
  if (!tab) return;
  const t = e.touches[0];
  hold(tab);
  press = { tab, x: t.clientX, y: t.clientY, timer: setTimeout(begin, HOLD_MS) };
}, { capture: true, passive: true });
document.addEventListener("touchmove", (e) => {
  const t = e.touches[0];
  if (!t) return;
  if (press && Math.hypot(t.clientX - press.x, t.clientY - press.y) > SLOP_PX) cancelPress();
  if (!drag) return;
  e.preventDefault();  // no scrolling while a tab is carried
  drag.x = t.clientX; drag.y = t.clientY;
  const el = document.elementFromPoint(drag.x, drag.y);
  if (el !== drag.over) {
    if (drag.over) fire(drag.over, "dragleave", drag.x, drag.y, drag.dt);
    fire(el, "dragenter", drag.x, drag.y, drag.dt);
    drag.over = el;
  }
  fire(el, "dragover", drag.x, drag.y, drag.dt);
}, { capture: true, passive: false });
document.addEventListener("touchend", (e) => {
  cancelPress();
  if (drag) { e.preventDefault(); finish(true); }  // preventDefault: no click after a drop
}, { capture: true, passive: false });
document.addEventListener("touchcancel", () => { cancelPress(); finish(false); }, { capture: true });
// The long press must not open the WebView's context menu or select text.
document.addEventListener("contextmenu", (e) => {
  if (drag || (e.target && e.target.closest && e.target.closest(".wb-tab") && press)) e.preventDefault();
}, true);
"#;
