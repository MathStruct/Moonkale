//! The desktop presence client's wire format against a running hub
//! (Milestone 9). Ignored unless `MOONKALE_HUB` points at a server, e.g.
//! `MOONKALE_HUB=http://127.0.0.1:8090 cargo test -p desktop --test hub -- --ignored`.

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

#[tokio::test]
#[ignore]
async fn join_and_see_myself() {
    let hub = std::env::var("MOONKALE_HUB").expect("MOONKALE_HUB");
    let url = format!(
        "{}/api/presence",
        hub.replacen("http://", "ws://", 1).trim_end_matches('/')
    );
    let mut req = url.clone().into_client_request().unwrap();
    if let Ok(t) = std::env::var("MOONKALE_TOKEN") {
        req.headers_mut()
            .insert("authorization", format!("Bearer {t}").parse().unwrap());
    }
    let (socket, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("connect");
    let (mut sink, mut stream) = socket.split();
    let join = serde_json::json!({ "kind": "join", "room": "test-room", "member": { "window": "native-test", "name": "Native Tester", "active": "README.md", "line": 3 } });
    sink.send(Message::Binary(serde_json::to_vec(&join).unwrap().into()))
        .await
        .unwrap();
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .expect("reply")
        .unwrap()
        .unwrap();
    let text = match msg {
        Message::Binary(b) => String::from_utf8(b.to_vec()).unwrap(),
        Message::Text(t) => t.to_string(),
        other => panic!("{other:?}"),
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["kind"], "members");
    let names: Vec<&str> = v["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Native Tester"), "{text}");
    let me = v["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["window"] == "native-test")
        .unwrap();
    assert_eq!(me["line"], 3);
}
