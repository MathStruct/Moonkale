//! Manual, trusted-input RAF recording in the production desktop WebView.
use dioxus::prelude::*;

pub fn initial_document() -> Option<(String, String)> {
    let path = std::path::PathBuf::from(std::env::var_os("MOONKALE_FPS_SOURCE")?);
    assert!(std::fs::metadata(&path).expect("FPS source metadata").len() <= 8 * 1024 * 1024);
    let name = path.file_name()?.to_str()?.to_owned();
    Some((
        name,
        std::fs::read_to_string(path).expect("read FPS source"),
    ))
}

#[component]
pub fn FrameRecorder() -> Element {
    let mut gesture = use_signal(|| "typing".to_owned());
    let mut recording = use_signal(|| false);
    let mut status =
        use_signal(|| "Choose a gesture, then record and interact for five seconds.".to_owned());
    rsx! {
        style { ".desktop-fixture .canonical {{ display: none !important; }} .desktop-fixture .harness-editor {{ width: 100%; height: calc(100vh - 110px); }} .fps-recorder {{ padding: 8px; white-space: normal; }}" }
        div { class: "fps-recorder",
            select { value: gesture, disabled: recording(), oninput: move |event| gesture.set(event.value()),
                for name in ["typing", "held-arrows", "wheel", "small-scroll-deltas", "scrollbar-drag", "selection-drag", "caret-movement"] {
                    option { value: name, "{name}" }
                }
            }
            button { disabled: recording(), onclick: move |_| {
                if recording() { return; }
                let selected = gesture();
                recording.set(true);
                status.set("Waiting for a real editor gesture; recording starts with that gesture.".into());
                spawn(async move {
                    let result = async {
                        let mut eval = document::eval(include_str!("fps-recorder.js"));
                        eval.send(serde_json::json!({"gesture": selected, "durationMs": 5000, "build": if cfg!(debug_assertions) {"dev-desktop"} else {"release-desktop"}})).map_err(|error| error.to_string())?;
                        let sample: serde_json::Value = eval.recv().await.map_err(|error| error.to_string())?;
                        let path = std::path::PathBuf::from(std::env::var_os("MOONKALE_FPS_REPORT").ok_or("missing report path")?);
                        let mut report = std::fs::metadata(&path).ok().filter(|metadata| metadata.len() <= 1_000_000)
                            .and_then(|_| std::fs::read(&path).ok())
                            .and_then(|bytes| serde_json::from_slice::<Vec<serde_json::Value>>(&bytes).ok()).unwrap_or_default();
                        if report.len() >= 100 { report.remove(0); }
                        report.push(sample.clone());
                        std::fs::write(path, serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
                        Ok::<_,String>(sample)
                    }.await;
                    match result {
                        Ok(sample) => status.set(format!("{}: p95 {} ms, max {} ms, {} frames; passed: {}. Saved to MOONKALE_FPS_REPORT.", selected, sample["p95"], sample["max"], sample["frames"], sample["passed"])),
                        Err(error) => status.set(format!("Recorder failed: {error}")),
                    }
                    recording.set(false);
                });
            }, "Record trusted input" }
            span { "{status}" }
        }
    }
}
