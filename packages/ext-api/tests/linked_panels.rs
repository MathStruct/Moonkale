//! Link invariants and backward-readable workspace records.
use moonkale_core::NodeId;
use moonkale_ext_api::{links::*, workspace::SavedWorkspace};
use moonkale_state::{decode, encode, Record};

fn group() -> LinkGroup {
    let node = NodeId::fresh("linked-test");
    LinkGroup {
        agent: None,
        id: "test".into(),
        colour: 3,
        number: 3,
        members: vec![
            ("lead".into(), Role::Lead, Follow::Group),
            ("graph:test".into(), Role::Follower, Follow::Group),
            (
                "pin".into(),
                Role::Follower,
                Follow::Pinned(Subject::new(node)),
            ),
        ],
        subject: Subject::new(node),
        history: vec![Subject::new(node)],
        cursor: 0,
    }
}
#[test]
fn closing_lead_closes_group_followers_but_keeps_pins_and_colour() {
    let mut g = group();
    assert_eq!(g.close("lead"), ["graph:test"]);
    assert_eq!(g.members.len(), 1);
    assert_eq!(g.members[0].0, "pin");
    assert_eq!(g.colour, 3);
    assert_eq!(g.marker(), "③");
}
#[test]
fn closing_follower_keeps_lead_and_other_followers() {
    let mut g = group();
    assert!(g.close("graph:test").is_empty());
    assert_eq!(g.members.len(), 2);
    assert!(g.members.iter().any(|(_, r, _)| *r == Role::Lead));
    g.number = 9;
    assert_eq!(g.marker(), "(9)");
}
#[test]
fn v1_workspace_migrates_and_v2_preserves_groups_pins_and_history() {
    let old = serde_json::json!({"v":1,"data":{
        "id":"old","name":"Old","touched":1,"sources":[],"documents":[],"active":null,"settings":{},"layout":{}
    }});
    let mut saved: SavedWorkspace = decode(&serde_json::to_vec(&old).unwrap()).unwrap();
    assert!(saved.link_groups.is_empty());
    assert!(saved.link_views.is_empty());
    saved.link_groups.push(group());
    let g = &saved.link_groups[0];
    saved.link_views.insert(
        "graph:test".into(),
        LinkView {
            camera: Some([1., 2., 3., 0., 0., 0., 20.]),
            detached: g.subject.clone(),
        },
    );
    assert_eq!(SavedWorkspace::VERSION, 2);
    assert_eq!(decode::<SavedWorkspace>(&encode(&saved)).unwrap(), saved);
    let mut future = old;
    future["v"] = serde_json::json!(3);
    assert!(decode::<SavedWorkspace>(&serde_json::to_vec(&future).unwrap()).is_err());
}

#[test]
fn exchange_refuses_invalid_group_and_camera_metadata() {
    let mut saved: SavedWorkspace = serde_json::from_value(serde_json::json!({
        "id":"bounded","name":"Bounded","touched":1,"sources":[],"documents":[],"active":null,"settings":{},"layout":{}
    })).unwrap();
    saved.link_groups.push(group());
    assert!(saved.validate_links().is_ok());
    saved.link_groups[0].colour = 9;
    assert!(saved.validate_links().is_err());
    saved.link_groups[0].colour = 3;
    saved.link_groups[0].cursor = 256;
    assert!(saved.validate_links().is_err());
    saved.link_groups[0].cursor = 0;
    saved.link_groups[0]
        .members
        .push(("lead".into(), Role::Follower, Follow::Group));
    assert!(saved.validate_links().is_err());
    saved.link_groups[0].members.pop();
    saved.link_views.insert(
        "graph:test".into(),
        LinkView {
            camera: Some([0.; 7]),
            detached: Subject::new(NodeId::fresh("bounded")),
        },
    );
    assert!(saved.validate_links().is_err());
}
