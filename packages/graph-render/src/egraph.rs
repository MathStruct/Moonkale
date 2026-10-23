//! E-graph presentation over ordinary engine groups, nodes, layers and deltas.
use crate::graph::{InDelta, InGraph};
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};

/// A bounded canonical snapshot plus the reader's collapsed classes.
/// Class ids name ordinary anchor nodes; children target those anchors.
#[derive(Clone)]
pub struct EGraph {
    input: InGraph,
    collapsed: HashSet<String>,
    shown: InGraph,
}

impl EGraph {
    /// Parse engine JSON with `groups[].kind = "eclass"` and an optional
    /// `representative` member id. No producer-specific schema is required.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 4 * 1024 * 1024 {
            return Err("e-graph exceeds 4 MiB".into());
        }
        let mut input: InGraph = serde_json::from_str(text).map_err(|_| "invalid e-graph JSON")?;
        if input.nodes.len() > 4096 || input.edges.len() > 32768 || input.groups.len() > 4096 {
            return Err("e-graph exceeds element limits".into());
        }
        let ids: HashSet<_> = input.nodes.iter().map(|n| n.id.as_str()).collect();
        if ids.len() != input.nodes.len()
            || input
                .edges
                .iter()
                .any(|e| e.a >= input.nodes.len() || e.b >= input.nodes.len() || e.id.is_none())
        {
            return Err("e-graph requires unique node ids, edge ids and valid endpoints".into());
        }
        let edge_ids: HashSet<_> = input.edges.iter().map(|e| &e.id).collect();
        let group_ids: HashSet<_> = input.groups.iter().map(|g| &g.id).collect();
        if edge_ids.len() != input.edges.len() || group_ids.len() != input.groups.len() {
            return Err("duplicate edge or group id".into());
        }
        let mut members = HashSet::new();
        let classes: HashSet<_> = input
            .groups
            .iter()
            .filter(|g| g.kind == "eclass")
            .map(|g| g.id.as_str())
            .collect();
        if classes.is_empty() {
            return Err("no e-classes".into());
        }
        for g in input.groups.iter().filter(|g| g.kind == "eclass") {
            if !ids.contains(g.id.as_str())
                || g.members.is_empty()
                || g.members.iter().any(|id| {
                    !ids.contains(id.as_str())
                        || classes.contains(id.as_str())
                        || !members.insert(id.clone())
                })
                || g.representative
                    .as_ref()
                    .is_some_and(|id| !g.members.contains(id))
            {
                return Err("invalid e-class anchor, membership or representative".into());
            }
        }
        if input.edges.iter().any(|e| {
            e.kind == "child"
                && (!members.contains(&input.nodes[e.a].id)
                    || !classes.contains(input.nodes[e.b].id.as_str()))
        }) {
            return Err("child edges must connect e-nodes to e-class anchors".into());
        }
        // Provenance is an ordinary engine layer, independently toggleable.
        for edge in &mut input.edges {
            if edge.kind == "provenance" {
                edge.layers = vec!["provenance".into()];
            }
        }
        if !input.layers.iter().any(|l| l.id == "provenance") {
            input.layers.push(
                serde_json::from_value(json!({"id":"provenance", "label":"provenance"})).unwrap(),
            );
        }
        let mut this = Self {
            shown: input.clone(),
            input,
            collapsed: HashSet::new(),
        };
        this.shown = this.project();
        Ok(this)
    }

    /// Current ordinary engine snapshot (also useful for non-browser hosts).
    pub fn graph(&self) -> &InGraph {
        &self.shown
    }

    /// Class ids and labels for a host's per-class controls.
    pub fn classes(&self) -> Vec<(String, String)> {
        self.input
            .groups
            .iter()
            .filter(|g| g.kind == "eclass")
            .map(|g| (g.id.clone(), g.label.clone()))
            .collect()
    }

    /// Follow a now-hidden member to its aggregate, retaining visible ids.
    pub fn visible_id(&self, id: &str) -> String {
        self.proxy()
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string())
    }

    fn proxy(&self) -> HashMap<String, String> {
        self.input
            .groups
            .iter()
            .filter(|g| self.collapsed.contains(&g.id))
            .flat_map(|g| g.members.iter().map(|m| (m.clone(), g.id.clone())))
            .collect()
    }

    fn project(&self) -> InGraph {
        let proxies = self.proxy();
        let mut out = self.input.clone();
        out.nodes.retain(|n| !proxies.contains_key(&n.id));
        for g in &mut out.groups {
            if g.kind != "eclass" {
                continue;
            }
            if self.collapsed.contains(&g.id) {
                let rep = g.representative.as_ref().unwrap_or(&g.members[0]);
                let original = self.input.nodes.iter().find(|n| &n.id == rep).unwrap();
                let anchor = out.nodes.iter_mut().find(|n| n.id == g.id).unwrap();
                // Keep the stable class endpoint/position, display the chosen term.
                anchor.label.clone_from(&original.label);
                anchor.kind.clone_from(&original.kind);
                anchor.caption = Some(g.label.clone());
                g.members.clear();
            }
            g.members.push(g.id.clone());
        }
        let indices: HashMap<_, _> = out
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        for e in &mut out.edges {
            let a = &self.input.nodes[e.a].id;
            let b = &self.input.nodes[e.b].id;
            e.a = indices[proxies.get(a).unwrap_or(a).as_str()];
            e.b = indices[proxies.get(b).unwrap_or(b).as_str()];
        }
        out
    }

    /// Collapse one named class, or every class when `id` is empty. Positions
    /// come from the live engine so expand restores dragged member locations.
    /// Returns the existing aggregate/proxy delta format; camera is untouched.
    pub fn collapse(
        &mut self,
        id: &str,
        collapsed: bool,
        positions: &HashMap<String, [f32; 2]>,
    ) -> Result<InDelta, String> {
        if !id.is_empty()
            && !self
                .input
                .groups
                .iter()
                .any(|g| g.id == id && g.kind == "eclass")
        {
            return Err("unknown e-class".into());
        }
        for n in &mut self.input.nodes {
            if let Some(p) = positions.get(&n.id) {
                n.pos = Some(*p);
            }
        }
        for g in &self.input.groups {
            if g.kind == "eclass" && (id.is_empty() || g.id == id) {
                if collapsed {
                    self.collapsed.insert(g.id.clone());
                } else {
                    self.collapsed.remove(&g.id);
                }
            }
        }
        let next = self.project();
        let old: HashSet<_> = self.shown.nodes.iter().map(|n| &n.id).collect();
        let new: HashSet<_> = next.nodes.iter().map(|n| &n.id).collect();
        let mut edges = Vec::new();
        for e in &next.edges {
            let mut v = serde_json::to_value(e).unwrap();
            v["a"] = json!(next.nodes[e.a].id);
            v["b"] = json!(next.nodes[e.b].id);
            edges.push(v);
        }
        let delta = serde_json::from_value(json!({
            "remove_nodes": old.difference(&new).collect::<Vec<_>>(),
            "add_nodes": next.nodes,
            "remove_edges": self.shown.edges.iter().map(|e| &e.id).collect::<Vec<_>>(),
            "add_edges": edges,
            "add_groups": next.groups,
        }))
        .map_err(|_| "could not create e-graph delta")?;
        self.shown = next;
        Ok(delta)
    }

    /// All incoming rewrite edges, including shared/cyclic ancestry, in O(V+E).
    /// Endpoints are projected onto collapsed class representatives.
    pub fn ancestry(&self, id: &str) -> (HashSet<String>, HashSet<String>) {
        let proxies = self.proxy();
        let mut incoming: HashMap<&str, Vec<_>> = HashMap::new();
        for e in &self.input.edges {
            if e.kind == "provenance" {
                incoming
                    .entry(&self.input.nodes[e.b].id)
                    .or_default()
                    .push(e);
            }
        }
        let mut queue = VecDeque::from([id.to_string()]);
        if self.collapsed.contains(id) {
            if let Some(g) = self.input.groups.iter().find(|g| g.id == id) {
                queue =
                    VecDeque::from([g.representative.as_ref().unwrap_or(&g.members[0]).clone()]);
            }
        }
        let mut visited = HashSet::new();
        let mut edges = HashSet::new();
        while let Some(id) = queue.pop_front() {
            if !visited.insert(id.clone()) {
                continue;
            }
            for e in incoming.get(id.as_str()).into_iter().flatten() {
                edges.insert(e.id.clone().unwrap());
                queue.push_back(self.input.nodes[e.a].id.clone());
            }
        }
        (
            visited
                .into_iter()
                .map(|id| proxies.get(&id).cloned().unwrap_or(id))
                .collect(),
            edges,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    const FIXTURE: &str = include_str!("../fixtures/egraph.json");

    #[test]
    fn collapse_expand_keeps_endpoints_positions_and_styles() {
        let mut e = EGraph::parse(FIXTURE).unwrap();
        let original = e.graph().clone();
        let mut graph = Graph::from_input(original.clone());
        let positions = HashMap::from([("a".into(), [111.0, 222.0])]);
        let delta = e.collapse("result", true, &positions).unwrap();
        graph.apply_delta(delta);
        assert_eq!(graph.nodes.len(), original.nodes.len() - 4);
        assert_eq!(graph.edges.len(), original.edges.len());
        assert!(e.graph().nodes.iter().all(|n| n.id != "a"));
        assert_eq!(
            e.graph()
                .nodes
                .iter()
                .find(|n| n.id == "result")
                .unwrap()
                .label,
            "a"
        );
        assert!(graph
            .edges
            .iter()
            .all(|edge| edge.a < graph.nodes.len() && edge.b < graph.nodes.len()));
        graph.apply_delta(e.collapse("result", false, &HashMap::new()).unwrap());
        assert_eq!(graph.nodes.len(), original.nodes.len());
        assert_eq!(
            e.graph().nodes.iter().find(|n| n.id == "a").unwrap().pos,
            Some([111.0, 222.0])
        );
        for (a, b) in original.edges.iter().zip(&e.graph().edges) {
            assert_eq!(a.label, b.label);
            assert_eq!(a.id, b.id);
        }
        graph.apply_delta(e.collapse("", true, &HashMap::new()).unwrap());
        assert_eq!(graph.nodes.len(), 4);
        graph.apply_delta(e.collapse("", false, &HashMap::new()).unwrap());
        assert_eq!(graph.nodes.len(), original.nodes.len());
    }

    #[test]
    fn ancestry_is_incoming_shared_and_cycle_safe() {
        let mut e = EGraph::parse(FIXTURE).unwrap();
        let (nodes, edges) = e.ancestry("a");
        assert_eq!(nodes.len(), 6);
        assert_eq!(
            edges,
            HashSet::from([
                "rewrite-assoc".into(),
                "rewrite-congruence".into(),
                "rewrite-one".into(),
                "rewrite-div".into(),
                "rewrite-ratio-use".into()
            ])
        );
        let mut cycle = e
            .input
            .edges
            .iter()
            .find(|e| e.id.as_deref() == Some("rewrite-one"))
            .unwrap()
            .clone();
        std::mem::swap(&mut cycle.a, &mut cycle.b);
        cycle.id = Some("cycle".into());
        e.input.edges.push(cycle);
        assert_eq!(e.ancestry("a").1.len(), 6);
        e.collapse("result", true, &HashMap::new()).unwrap();
        assert_eq!(
            e.ancestry("result").0,
            HashSet::from(["result".into(), "ratio".into(), "1".into()])
        );
    }

    #[test]
    fn invalid_members_endpoints_ids_and_size_are_refused() {
        for field in ["members", "representative", "id"] {
            let mut v: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
            v["groups"][0][field] = if field == "members" {
                json!(["missing"])
            } else {
                json!("missing")
            };
            assert!(EGraph::parse(&v.to_string()).is_err());
        }
        assert!(EGraph::parse(&" ".repeat(4 * 1024 * 1024 + 1)).is_err());
        let mut v: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        v["edges"][0]["b"] = json!(999999);
        assert!(EGraph::parse(&v.to_string()).is_err());
    }
}
