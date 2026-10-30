//! Always-mounted sharing controls own the indicator and publication clock.
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Command, Workspace};

#[component]
pub fn Sharing(mut ws: Workspace) -> Element {
    let cleanup = use_hook(move || {
        let mut events = document::eval(
            r#"
            const tick = setInterval(() => dioxus.send(false), 50);
            const cancel = e => {
                if (e.type === 'keydown' && e.key !== 'Escape' && !['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','PageUp','PageDown','Home','End','Tab'].includes(e.key)) return;
                if (e.key !== 'Escape' && e.target.closest?.('.mk-sharing')) return;
                dioxus.send(true);
            };
            document.addEventListener('keydown', cancel, true);
            document.addEventListener('pointerdown', cancel, true);
            document.addEventListener('wheel', cancel, {capture:true,passive:true});
            await dioxus.recv();
            clearInterval(tick);
            document.removeEventListener('keydown', cancel, true);
            document.removeEventListener('pointerdown', cancel, true);
            document.removeEventListener('wheel', cancel, true);
        "#,
        );
        let cleanup = events;
        spawn(async move {
            while let Ok(cancel) = events.recv::<bool>().await {
                if cancel && ws.session.sharing.peek().following.is_some() {
                    ws.session.sharing.with_mut(|s| s.following = None);
                }
                ws.sharing_tick();
            }
        });
        cleanup
    });
    use_drop(move || {
        let _ = cleanup.send(true);
    });
    let state = ws.session.sharing.read().clone();
    let _ = ws.session.presence.read();
    let others = ws.others();
    let active = state.cursor || state.pointer || state.agent_enabled;
    rsx! {
        span { class: "mk-sharing-announcement mk-sr-only", role: "status", aria_live: "polite", aria_atomic: "true" }
        details { class: "mk-sharing", "data-sharing": active.to_string(),
            summary { class: "mk-menu-button", title: t!(ws, L, "sharing-title"),
                if active { span { class: "mk-sharing-indicator", "● " } }
                {t!(ws, L, "sharing-title")}
            }
            div { class: "mk-sharing-menu",
                button { class: "mk-btn", "data-command": "sharing.cursor", aria_pressed: state.cursor.to_string(), onclick: move |_| ws.dispatch(Command::ShareCursor), {t!(ws, L, "sharing-cursor")} }
                button { class: "mk-btn", "data-command": "sharing.pointer", aria_pressed: state.pointer.to_string(), onclick: move |_| ws.dispatch(Command::SharePointer), {t!(ws, L, "sharing-pointer")} }
                button { class: "mk-btn", "data-command": "sharing.agentCursor", aria_pressed: state.agent_enabled.to_string(), onclick: move |_| ws.dispatch(Command::ShareAgentCursor), {t!(ws, L, "sharing-agent-cursor")} }
                button { class: "mk-btn", "data-command": "sharing.stop", onclick: move |_| ws.dispatch(Command::StopSharing), {t!(ws, L, "sharing-stop")} }
                button { class: "mk-btn", "data-command": "sharing.announce", onclick: move |_| ws.dispatch(Command::AnnounceCursor), {t!(ws, L, "sharing-announce")} }
                for member in others.into_iter().filter(|m| m.sharing.as_ref().is_some_and(|p| p.cursor.is_some())) {
                    button { key: "{member.window}", class: "mk-btn mk-sharing-follow", aria_pressed: (state.following.as_ref() == Some(&member.window)).to_string(),
                        onclick: move |_| { let id = member.window.clone(); ws.session.sharing.with_mut(|s| { s.following = if s.following.as_ref() == Some(&id) { None } else { Some(id) }; s.follow_request = None; s.follow_opened = 0; s.follow_read_at = 0; }); },
                        {t!(ws, L, "sharing-follow", name = member.name.clone())}
                    }
                }
            }
        }
    }
}
