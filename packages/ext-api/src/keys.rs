//! The platform's **primary modifier** (spec 027, Prompt25): `Ctrl` in a
//! keybinding means Cmd on macOS and Ctrl everywhere else — never both.
//! Before this, `Ctrl` matched *either* modifier, so on a Mac `Ctrl+O`
//! opened a folder and `Ctrl+N` a file, stealing the Emacs-style cursor
//! keys macOS users expect in every text field.

use dioxus::prelude::Modifiers;

/// Is this a macOS host (desktop) or a Mac browser (web)?
pub fn is_mac() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        thread_local! {
            static MAC: bool = detect_mac_browser();
        }
        return MAC.with(|m| *m);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        cfg!(target_os = "macos")
    }
}

#[cfg(target_arch = "wasm32")]
fn detect_mac_browser() -> bool {
    use wasm_bindgen::JsValue;
    let global = js_sys::global();
    let Ok(nav) = js_sys::Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    let text = ["platform", "userAgent"]
        .iter()
        .filter_map(|k| js_sys::Reflect::get(&nav, &JsValue::from_str(k)).ok())
        .filter_map(|v| v.as_string())
        .collect::<Vec<_>>()
        .join(" ");
    text.contains("Mac") || text.contains("iPhone") || text.contains("iPad")
}

/// The primary modifier of a keyboard event: Cmd on a Mac, Ctrl elsewhere.
/// Use this wherever a binding says `Ctrl`.
pub fn primary(m: &Modifiers) -> bool {
    primary_of(is_mac(), m.ctrl(), m.meta())
}

/// The pure rule behind [`primary`].
pub fn primary_of(mac: bool, ctrl: bool, meta: bool) -> bool {
    if mac {
        meta
    } else {
        ctrl
    }
}

/// How the primary modifier is written for this platform (`Cmd` / `Ctrl`).
pub fn primary_name() -> &'static str {
    if is_mac() {
        "Cmd"
    } else {
        "Ctrl"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_uses_cmd_only_and_others_ctrl_only() {
        assert!(primary_of(true, false, true));
        assert!(
            !primary_of(true, true, false),
            "Ctrl on a Mac is the app's, not ours"
        );
        assert!(primary_of(false, true, false));
        assert!(
            !primary_of(false, false, true),
            "the Windows/Super key is not Ctrl"
        );
    }
}
