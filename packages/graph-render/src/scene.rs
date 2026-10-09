//! The renderer as a Rust library (Milestone 18 phase 6.1): **graph in,
//! events out**, with no browser and no Moonkale types. A Lenticulum or
//! Sophia viewer — or a test — builds a [`Scene`] from an [`InGraph`], steps
//! the force layout, fits the camera, and turns pointer positions into
//! [`Event`]s. The wasm module (`web`) drives the same pieces and emits the
//! same events as JSON (`{"kind": "hover", …}`), so a page and a Rust host
//! see one protocol.
//!
//! ```
//! use moonkale_graph_render::graph::{InEdge, InGraph, InNode};
//! use moonkale_graph_render::scene::{Event, Scene};
//!
//! let node = |id: &str| InNode { id: id.into(), label: id.into(), kind: "file".into(), key: id.into(), color: None, layers: Vec::new() };
//! let mut scene = Scene::new(InGraph {
//!     nodes: vec![node("a"), node("b"), node("c")],
//!     edges: vec![InEdge { a: 0, b: 1, kind: "link".into(), color: None, ..Default::default() }],
//!     ..Default::default()
//! });
//! scene.resize(800.0, 600.0);
//! scene.settle(500);
//! scene.fit();
//! let (x, y) = scene.screen_position("a").unwrap();
//! assert!(matches!(scene.pointer_move(x, y), Some(Event::Hover { id: Some(id), .. }) if id == "a"));
//! assert_eq!(scene.click(x, y, false), Some(Event::Click { id: "a".into() }));
//! ```

use crate::camera::Camera;
use crate::graph::{Graph, InGraph, Node};
use crate::layout::Layout;
use serde::{Deserialize, Serialize};

/// What the view reports — the same shapes the wasm module emits as JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    /// The pointer is over a node (`id: None`: over none any more).
    Hover {
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, rename = "nodeKind", skip_serializing_if = "Option::is_none")]
        node_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<f32>,
    },
    Click {
        id: String,
    },
    /// The selection changed (spec 031 §5) — the selected node and edge
    /// instance ids, sorted. The wasm module emits it on click picks; the
    /// native `Scene` gains picking with its stage-1 twin.
    Select {
        nodes: Vec<String>,
        edges: Vec<String>,
    },
    #[serde(rename = "dblclick")]
    DoubleClick {
        id: String,
    },
    /// The layout has come to rest.
    Settled,
}

/// A graph with its layout and camera.
pub struct Scene {
    pub graph: Graph,
    pub layout: Layout,
    pub camera: Camera,
    hovered: Option<usize>,
}

impl Scene {
    pub fn new(input: InGraph) -> Self {
        let graph = Graph::from_input(input);
        let layout = Layout::new(&graph);
        Self {
            graph,
            layout,
            camera: Camera::default(),
            hovered: None,
        }
    }

    /// From the JSON the wasm module's `set_graph` takes.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        Ok(Self::new(serde_json::from_str(json)?))
    }

    /// The viewport in CSS pixels.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.camera.width = width;
        self.camera.height = height;
    }

    /// One layout step; [`Event::Settled`] when the layout came to rest.
    pub fn step(&mut self) -> Option<Event> {
        if !self.layout.running {
            return None;
        }
        self.layout.step(&mut self.graph);
        (!self.layout.running).then_some(Event::Settled)
    }

    /// Step until the layout rests or `max_steps` ran; returns the steps run.
    pub fn settle(&mut self, max_steps: u32) -> u32 {
        let mut n = 0;
        while self.layout.running && n < max_steps {
            self.layout.step(&mut self.graph);
            n += 1;
        }
        n
    }

    /// Fit the whole graph into the viewport.
    pub fn fit(&mut self) {
        self.camera.fit(&self.graph, 40.0);
    }

    /// The node under a screen point (nearest wins).
    pub fn node_at(&self, x: f32, y: f32) -> Option<&Node> {
        self.camera
            .hit(&self.graph, x, y)
            .map(|i| &self.graph.nodes[i])
    }

    /// Where a node is on screen.
    pub fn screen_position(&self, id: &str) -> Option<(f32, f32)> {
        let n = self.graph.nodes.iter().find(|n| n.id == id)?;
        if self.camera.three_d {
            self.camera.project(n.x, n.y, n.z).map(|(x, y, _)| (x, y))
        } else {
            Some(self.camera.world_to_screen(n.x, n.y))
        }
    }

    /// The pointer moved: a [`Event::Hover`] when the node under it changed.
    pub fn pointer_move(&mut self, x: f32, y: f32) -> Option<Event> {
        let hit = self.camera.hit(&self.graph, x, y);
        if hit == self.hovered {
            return None;
        }
        self.hovered = hit;
        Some(match hit {
            Some(i) => {
                let n = &self.graph.nodes[i];
                Event::Hover {
                    id: Some(n.id.clone()),
                    label: Some(n.label.clone()),
                    node_kind: Some(n.kind.clone()),
                    key: Some(n.key.clone()),
                    x: Some(x),
                    y: Some(y),
                }
            }
            None => Event::Hover {
                id: None,
                label: None,
                node_kind: None,
                key: None,
                x: None,
                y: None,
            },
        })
    }

    /// A click (or double click) at a screen point.
    pub fn click(&self, x: f32, y: f32, double: bool) -> Option<Event> {
        let id = self.node_at(x, y)?.id.clone();
        Some(if double {
            Event::DoubleClick { id }
        } else {
            Event::Click { id }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The events serialize exactly as the wasm module's JSON.
    #[test]
    fn events_are_the_wasm_protocol() {
        let j = |e: &Event| serde_json::to_value(e).unwrap();
        assert_eq!(
            j(&Event::Click { id: "a".into() }),
            serde_json::json!({"kind": "click", "id": "a"})
        );
        assert_eq!(
            j(&Event::DoubleClick { id: "a".into() }),
            serde_json::json!({"kind": "dblclick", "id": "a"})
        );
        assert_eq!(j(&Event::Settled), serde_json::json!({"kind": "settled"}));
        let off = Event::Hover {
            id: None,
            label: None,
            node_kind: None,
            key: None,
            x: None,
            y: None,
        };
        assert_eq!(j(&off), serde_json::json!({"kind": "hover", "id": null}));
        let on: Event = serde_json::from_value(serde_json::json!({"kind": "hover", "id": "n", "label": "N", "nodeKind": "file", "key": "n.md", "x": 1.0, "y": 2.0})).unwrap();
        assert!(matches!(on, Event::Hover { node_kind: Some(k), .. } if k == "file"));
    }

    #[test]
    fn hover_reports_changes_only() {
        let mut s = Scene::from_json(
            r#"{"nodes":[{"id":"a","label":"a","kind":"file","key":"a"}],"edges":[]}"#,
        )
        .unwrap();
        s.resize(400.0, 300.0);
        s.settle(100);
        s.fit();
        let (x, y) = s.screen_position("a").unwrap();
        assert!(s.pointer_move(x, y).is_some());
        assert!(s.pointer_move(x + 0.5, y).is_none(), "same node: no event");
        assert!(matches!(
            s.pointer_move(-1000.0, -1000.0),
            Some(Event::Hover { id: None, .. })
        ));
        assert_eq!(s.click(-1000.0, -1000.0, false), None);
    }
}
