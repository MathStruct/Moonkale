// Host-side navigation over the engine's existing aggregate/proxy delta path.
// Source queries own membership; the engine owns coordinates and the camera.
let coarseGraph = null;
let coarseExpanded = null;
function coarseCollapse() {
    if (!coarseExpanded) return;
    const {node, position, members, edges} = coarseExpanded;
    view.hold_camera();
    view.apply_delta(JSON.stringify({remove_nodes:members, remove_groups:["coarse-region"], add_nodes:[{...node,pos:position,pinned:true}], add_edges:edges}));
    coarseExpanded = null;
    dioxus.send({kind:"graphCounts",nodes:view.node_count(),edges:view.edge_count()});
}
function coarseExpand(id, graph) {
    if (!coarseGraph || coarseExpanded) throw new Error("region unavailable");
    const node = coarseGraph.nodes.find(n=>n.id===id);
    if (!node || !graph.nodes.length || graph.nodes.length + coarseGraph.nodes.length - 1 > 100000) throw new Error("region too large");
    const positions = JSON.parse(view.node_positions());
    const position = positions[id];
    if (!position) return;
    const edges = coarseGraph.edges.flatMap((edge,i)=>{
        const a=coarseGraph.nodes[edge.a].id, b=coarseGraph.nodes[edge.b].id;
        return a===id || b===id ? [{...edge,id:`coarse-boundary-${i}`,a,b}] : [];
    });
    const radius = Math.max(60,Math.sqrt(graph.nodes.length)*8);
    const members=graph.nodes.map(n=>n.id);
    // Keep the context fixed; only the newly opened region participates in
    // force layout. Starting coordinates are distributed around its anchor.
    const context = coarseGraph.nodes.filter(n=>n.id!==id).map(n=>({...n,pos:positions[n.id],pinned:true}));
    const nodes = graph.nodes.map((n,i)=>{
        const angle=i*2.399963229728653;
        const r=radius*Math.sqrt((i+.5)/graph.nodes.length);
        return {...n,pos:[position[0]+r*Math.cos(angle),position[1]+r*Math.sin(angle)]};
    });
    const links = graph.edges.map((e,i)=>({...e,id:`coarse-region-${i}`,a:members[e.a],b:members[e.b]}));
    // Boundary relations remain proxy edges until the source supplies finer
    // boundary membership. They reconnect to a stable region representative.
    links.push(...edges.map(e=>({...e,a:e.a===id?members[0]:e.a,b:e.b===id?members[0]:e.b})));
    view.hold_camera();
    view.apply_delta(JSON.stringify({remove_nodes:[id],add_nodes:[...context,...nodes],add_edges:links,
        add_groups:[{id:"coarse-region",label:node.label,members}]}));
    coarseExpanded = {node,position,members,edges};
    dioxus.send({kind:"graphCounts",nodes:view.node_count(),edges:view.edge_count()});
}
