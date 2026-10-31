use crate::{model::*, L};
use dioxus::prelude::*;
use moonkale_core::{NodeId, QueryResult, Value};
use moonkale_ext_api::{t, Workspace};
use std::{collections::BTreeMap, rc::Rc};
const CSS: Asset = asset!("/assets/plot.css");
const W: f64 = 720.0;
const H: f64 = 360.0;
#[derive(Clone)]
struct Shared(Rc<QueryResult>);
impl PartialEq for Shared {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
#[derive(Clone, Copy, PartialEq)]
struct View {
    spec: Spec,
    zoom: f64,
    pan: (f64, f64),
}
#[derive(Clone, Copy)]
struct Memory(Signal<BTreeMap<(String, String), View>>);

#[component]
pub fn PlotPanel(ws: Workspace, panel: String) -> Element {
    let owner = ws.link_subject(&panel).map(|s| s.node);
    let data = owner.and_then(|o| ws.links.results.read().get(&o).cloned().map(|r| (o, r)));
    rsx! {
        moonkale_ext_api::Stylesheet {href:CSS}
        if let Some((owner,data))=data {
            if let Some(table)=&data.table {
                PlotData {key:"{owner}:{table.columns:?}",ws,panel,owner,data:Shared(data.clone())}
            } else { p {{t!(ws,L,"plot-empty")}} }
        } else {p {{t!(ws,L,"plot-empty")}}}
    }
}
fn tooltip(result: &QueryResult, row: usize) -> String {
    let Some(table) = result.table.as_ref() else {
        return String::new();
    };
    let Some(values) = table.rows.get(row) else {
        return String::new();
    };
    table
        .columns
        .iter()
        .zip(values)
        .take(32)
        .map(|(c, v)| {
            let value = match v {
                Value::Text(s) | Value::Ref { label: s, .. } => s.chars().take(120).collect(),
                Value::Vector(v) => format!(
                    "{:?} … ({})",
                    &v.as_slice()[..v.as_slice().len().min(8)],
                    v.as_slice().len()
                ),
                _ => v.to_string().chars().take(120).collect::<String>(),
            };
            format!("{}: {value}", c.chars().take(80).collect::<String>())
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[component]
fn PlotData(ws: Workspace, panel: String, owner: NodeId, data: Shared) -> Element {
    let table = data.0.table.as_ref().expect("table result");
    let memory = use_hook(|| {
        ws.shared_state(|| Memory(Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT)))
    });
    let memory_key = (
        ws.saved_sessions.active.peek().clone(),
        format!(
            "{panel}:{owner}:{}",
            NodeId::from_content(format!("{:?}", table.columns).as_bytes())
        ),
    );
    let mut view = use_signal(|| {
        memory.0.peek().get(&memory_key).copied().unwrap_or(View {
            spec: default_spec(table),
            zoom: 1.0,
            pan: (0.0, 0.0),
        })
    });
    use_effect({
        let key = memory_key.clone();
        move || {
            let current = *view.read();
            let mut memory = memory;
            if memory.0.peek().get(&key) != Some(&current) {
                memory.0.with_mut(|all| {
                    if all.len() >= 512 && !all.contains_key(&key) {
                        all.clear();
                    }
                    all.insert(key.clone(), current);
                });
            }
        }
    });
    let mut held = use_signal(|| data.clone());
    if *held.peek() != data {
        held.set(data.clone());
    }
    let spec = use_memo(move || view.read().spec);
    let prepared = use_memo(move || prepare(held.read().0.table.as_ref().unwrap(), spec()));
    // A table-selected row remains visible even when uniform sampling omitted it.
    let base_prepared = prepared;
    let selection_panel = panel.clone();
    let prepared = use_memo(move || {
        let current = held.read();
        let table = current.0.table.as_ref().unwrap();
        let s = spec();
        let base = base_prepared.read();
        let mut out = base.clone();
        if base.sampled && matches!(s.mark, Mark::Line | Mark::Scatter) {
            let selection = ws
                .link_subject(&selection_panel)
                .map(|s| s.selection)
                .unwrap_or_default();
            if let Some(row) = moonkale_ext_api::result::selected_row(owner, table, &selection) {
                if !out.points.iter().any(|p| p.row == row) {
                    if let Some((x, y)) = table.rows[row]
                        .get(s.x)
                        .and_then(number)
                        .zip(table.rows[row].get(s.y).and_then(number))
                    {
                        if s.xs.transform(x).is_some() && s.ys.transform(y).is_some() {
                            if out.points.len() >= POINT_CAP {
                                out.points.remove(out.points.len() / 2);
                            }
                            out.points.push(Point {
                                x,
                                y,
                                row,
                                members: vec![row],
                                count: 1,
                                label: String::new(),
                            });
                            if s.mark == Mark::Line {
                                out.points.sort_by(|a, b| a.x.total_cmp(&b.x));
                            }
                        }
                    }
                }
            }
        }
        out
    });
    let mut hovered = use_signal(|| None::<usize>);
    let mut active = use_signal(|| 0usize);
    let mut moved = use_signal(|| false);
    let mut drag = use_signal(|| None::<(f64, f64, (f64, f64))>);
    let mut size = use_signal(|| (900.0_f64, 500.0_f64));
    let v = view();
    let s = v.spec;
    let prepared_signal = prepared;
    let prepared = prepared.read();
    let points = &prepared.points;
    let selections_memo = use_memo(move || {
        let held = held.read();
        let table = held.0.table.as_ref().unwrap();
        prepared_signal
            .read()
            .points
            .iter()
            .map(|p| {
                let select =
                    moonkale_ext_api::result::row_selection(owner, p.row, &table.rows[p.row]);
                let members = p
                    .members
                    .iter()
                    .flat_map(|i| {
                        moonkale_ext_api::result::row_selection(owner, *i, &table.rows[*i])
                    })
                    .collect::<Vec<_>>();
                (select, members)
            })
            .collect::<Vec<_>>()
    });
    let links = selections_memo.read();
    let selections = links
        .iter()
        .map(|(select, _)| select.clone())
        .collect::<Vec<_>>();
    let selected = ws
        .link_subject(&panel)
        .map(|s| s.selection)
        .unwrap_or_default();
    let has_row = selected.iter().any(|id| id.starts_with("row:"));
    let highlighted = links
        .iter()
        .map(|(_, members)| {
            members
                .iter()
                .any(|id| (!has_row || id.starts_with("row:")) && selected.contains(id))
        })
        .collect::<Vec<_>>();
    let xs = if matches!(s.mark, Mark::Bar | Mark::Histogram) {
        Scale::Linear
    } else {
        s.xs
    };
    let ys = s.ys;
    let mut xa = Axis::from_values(points.iter().filter_map(|p| xs.transform(p.x)));
    if matches!(s.mark, Mark::Bar | Mark::Histogram) {
        let half = if s.mark == Mark::Bar {
            0.5
        } else {
            prepared.bar_width / 1.8
        };
        xa = Axis::from_values(points.iter().flat_map(|p| [p.x - half, p.x + half]));
    }
    let mut ya = Axis::from_values(points.iter().filter_map(|p| ys.transform(p.y)).chain(
        (matches!(s.mark, Mark::Bar | Mark::Histogram) && ys == Scale::Linear).then_some(0.0),
    ));
    if matches!(s.mark, Mark::Line | Mark::Scatter) {
        let px = (xa.hi - xa.lo) * 0.04;
        let py = (ya.hi - ya.lo) * 0.04;
        if (xa.lo - px).is_finite() && (xa.hi + px).is_finite() {
            xa.lo -= px;
            xa.hi += px;
        }
        if (ya.lo - py).is_finite() && (ya.hi + py).is_finite() {
            ya.lo -= py;
            ya.hi += py;
        }
    }
    if s.equal && ((ya.hi - ya.lo) * W / H).is_finite() {
        let span = (xa.hi - xa.lo).max((ya.hi - ya.lo) * W / H);
        let mx = xa.lo + (xa.hi - xa.lo) / 2.0;
        let my = ya.lo + (ya.hi - ya.lo) / 2.0;
        xa = Axis {
            lo: mx - span / 2.0,
            hi: mx + span / 2.0,
        };
        ya = Axis {
            lo: my - span * H / W / 2.0,
            hi: my + span * H / W / 2.0,
        };
    }
    xa = xa.viewport(v.pan.0, v.zoom);
    ya = ya.viewport(v.pan.1, v.zoom);
    let coords: Vec<_> = points
        .iter()
        .map(|p| {
            xs.transform(p.x)
                .zip(ys.transform(p.y))
                .map(|(x, y)| (100.0 + W * xa.unit(x), 410.0 - H * ya.unit(y)))
        })
        .collect();
    let polyline = coords
        .iter()
        .flatten()
        .map(|(x, y)| format!("{x:.3},{y:.3}"))
        .collect::<Vec<_>>()
        .join(" ");
    let partial = table.truncated || data.0.truncated || table.rows.len() > INPUT_CAP;
    let selected_point = highlighted.iter().position(|b| *b);
    let info = hovered()
        .or(selected_point)
        .and_then(|i| points.get(i))
        .map(|p| {
            let text = tooltip(&data.0, p.row);
            if p.count > 1 {
                format!(
                    "{}\n{}\n{text}",
                    p.label,
                    t!(ws, L, "plot-aggregate", count = p.count)
                )
            } else {
                text
            }
        })
        .unwrap_or_default();
    let x_label = table.columns.get(s.x).cloned().unwrap_or_default();
    let y_label = if s.mark == Mark::Histogram {
        t!(ws, L, "plot-count")
    } else {
        table.columns.get(s.y).cloned().unwrap_or_default()
    };
    let clip = format!("plot-clip-{}", NodeId::from_content(panel.as_bytes()));
    // SVG keeps one geometric coordinate system, while labels and hit targets
    // retain CSS-pixel sizes when the panel is narrow or letterboxed.
    let svg_scale = (size().0 / 900.0).min(size().1 / 500.0).max(0.01);
    let tick_font = 11.0 / svg_scale;
    let label_font = 14.0 / svg_scale;
    let x_label_cap = (W * svg_scale / 14.0).clamp(8.0, 50.0) as usize;
    let y_label_cap = (H * svg_scale / 14.0).clamp(8.0, 24.0) as usize;

    let x_ticks = if s.mark == Mark::Bar {
        let cap = (W * svg_scale / 140.0).floor().clamp(2.0, 8.0) as usize;
        decimate(prepared.categories.len(), cap)
            .into_iter()
            .map(|i| {
                (
                    i as f64,
                    prepared.categories[i].chars().take(10).collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
    } else {
        xa.ticks(
            W * svg_scale,
            if s.xs == Scale::Time { 190.0 } else { 100.0 },
        )
        .into_iter()
        .map(|tick| {
            (
                tick,
                if s.mark == Mark::Histogram {
                    s.xs.label(tick)
                } else {
                    xs.label(tick)
                },
            )
        })
        .collect()
    };
    let key_panel = panel.clone();
    let key_selections = selections.to_vec();
    let n = points.len();
    rsx! {
        div {class:"mk-plot", "data-mark":s.mark.id(), "data-points":"{n}", "data-partial":"{partial}", "data-zoom":"{v.zoom}",
            div {class:"mk-plot-toolbar",
            moonkale_ext_api::icons::ToolbarActions {
                primary:rsx!{button {class:"mk-btn mk-plot-fit",onclick:move |_|view.with_mut(|v|{v.zoom=1.0;v.pan=(0.0,0.0);}),{t!(ws,L,"plot-fit")}}},
                label {{t!(ws,L,"plot-x")}
                    select {class:"mk-plot-x",value:"{s.x}",onchange:move |e|{if let Ok(x)=e.value().parse() {view.with_mut(|v|v.spec.x=x);}},
                        for (i,c) in table.columns.iter().take(256).enumerate() {option {key:"{i}",value:"{i}","{c}"}}
                    }
                }
                label {{t!(ws,L,"plot-y")}
                    select {class:"mk-plot-y",value:"{s.y}",disabled:s.mark==Mark::Histogram,onchange:move |e|{if let Ok(y)=e.value().parse() {view.with_mut(|v|v.spec.y=y);}},
                        for (i,c) in table.columns.iter().take(256).enumerate() {option {key:"{i}",value:"{i}","{c}"}}
                    }
                }
                label {{t!(ws,L,"plot-mark")}
                    select {class:"mk-plot-mark",value:s.mark.id(),onchange:move |e|{if let Some(m)=Mark::ALL.into_iter().find(|m|m.id()==e.value()) {view.with_mut(|v|v.spec.mark=m);}},
                        for m in Mark::ALL {option {value:m.id(),{mark_name(ws,m)}}}
                    }
                }
                for (is_x,scale,label) in [(true,s.xs,t!(ws,L,"plot-x-scale")),(false,s.ys,t!(ws,L,"plot-y-scale"))] {
                    label {"{label}"
                        select {class:if is_x {"mk-plot-xs"} else {"mk-plot-ys"},value:scale.id(),disabled:is_x&&s.mark==Mark::Bar,
                            onchange:move |e|{if let Some(scale)=Scale::ALL.into_iter().find(|s|s.id()==e.value()) {view.with_mut(|v|{if is_x {v.spec.xs=scale;} else {v.spec.ys=scale;}});}},
                            for scale in Scale::ALL {option {value:scale.id(),{scale_name(ws,scale)}}}
                        }
                    }
                }
                label {input {class:"mk-plot-equal",r#type:"checkbox",checked:s.equal,onchange:move |e|view.with_mut(|v|v.spec.equal=e.checked())}{t!(ws,L,"plot-equal")}}
                for (factor,label) in [(1.5,t!(ws,L,"plot-zoom-in")),(1.0/1.5,t!(ws,L,"plot-zoom-out"))] {button {class:"mk-btn",onclick:move |_|view.with_mut(|v|v.zoom=(v.zoom*factor).clamp(0.25,1000.0)),"{label}"}}
                for (x,y,label) in [(-0.15,0.0,t!(ws,L,"plot-left")),(0.15,0.0,t!(ws,L,"plot-right")),(0.0,0.15,t!(ws,L,"plot-up")),(0.0,-0.15,t!(ws,L,"plot-down"))] {button {class:"mk-btn",onclick:move |_|view.with_mut(|v|{v.pan.0=(v.pan.0+x/v.zoom).clamp(-10.0,10.0);v.pan.1=(v.pan.1+y/v.zoom).clamp(-10.0,10.0);}),"{label}"}}
            }
            }
            p {class:"mk-plot-summary",role:"status",
                {t!(ws,L,"plot-loaded",rows=table.rows.len(),marks=n)}
                if partial {span {class:"mk-plot-partial",{t!(ws,L,"plot-partial")}}}
                if prepared.sampled {span {class:"mk-plot-sampled",{t!(ws,L,"plot-sampled",cap=POINT_CAP)}}}
                if prepared.omitted>0 {span {{t!(ws,L,"plot-omitted",count=prepared.omitted)}}}
            }
            p {class:"mk-plot-help",{t!(ws,L,"plot-help")}}
            svg {id:"plot-canvas-{panel}",class:"mk-plot-canvas",view_box:"0 0 900 500",preserve_aspect_ratio:"xMidYMid meet",tabindex:"0",role:"group","aria-label":t!(ws,L,"plot-title"),
                onmounted:move |e|{let el=e.data();moonkale_ext_api::crash::spawn(async move {let _=el.set_focus(true).await;});},
                onresize:move |e|{let z=e.get_content_box_size();if let Ok(z)=z {size.set((z.width.max(1.0),z.height.max(1.0)));}},
                onwheel:move |e:WheelEvent|{e.prevent_default();let factor=if e.delta().strip_units().y<0.0 {1.2} else {1.0/1.2};view.with_mut(|v|v.zoom=(v.zoom*factor).clamp(0.25,1000.0));},
                onpointerdown:move |e:PointerEvent|{if e.trigger_button()==Some(dioxus::html::input_data::MouseButton::Primary) {let c=e.client_coordinates();moved.set(false);drag.set(Some((c.x,c.y,view.peek().pan)));}},
                onpointermove:move |e:PointerEvent|{if let Some((x,y,pan))=*drag.peek() {let c=e.client_coordinates();if (c.x-x).abs()+(c.y-y).abs()>3.0 {moved.set(true);}let (width,height)=*size.peek();view.with_mut(|v|{v.pan.0=(pan.0-(c.x-x)/width*900.0/W/v.zoom).clamp(-10.0,10.0);v.pan.1=(pan.1+(c.y-y)/height*500.0/H/v.zoom).clamp(-10.0,10.0);});}},
                onpointerup:move |_|drag.set(None),onpointerleave:move |_|{drag.set(None);hovered.set(None);},
                onkeydown:move |e:KeyboardEvent|{
                    let mut at=(*active.peek()).min(n.saturating_sub(1));
                    match e.key() {
                        Key::ArrowLeft|Key::ArrowRight|Key::ArrowUp|Key::ArrowDown if e.modifiers().shift()=>{
                            let (x,y)=match e.key(){Key::ArrowLeft=>(-0.1,0.0),Key::ArrowRight=>(0.1,0.0),Key::ArrowUp=>(0.0,0.1),_=>(0.0,-0.1)};
                            view.with_mut(|v|{v.pan.0=(v.pan.0+x/v.zoom).clamp(-10.0,10.0);v.pan.1=(v.pan.1+y/v.zoom).clamp(-10.0,10.0);});
                        },
                        Key::ArrowLeft=>at=at.saturating_sub(1),Key::ArrowRight=>at=(at+1).min(n.saturating_sub(1)),Key::Home=>at=0,Key::End=>at=n.saturating_sub(1),
                        Key::Character(c) if c==" "=>{if let Some(ids)=key_selections.get(at) {ws.select_linked(&key_panel,ids.clone());}},
                        Key::Enter=>{if let Some(ids)=key_selections.get(at) {ws.select_linked(&key_panel,ids.clone());}},
                        Key::Character(c) if c=="+"||c=="="=>view.with_mut(|v|v.zoom=(v.zoom*1.2).min(1000.0)),
                        Key::Character(c) if c=="-"=>view.with_mut(|v|v.zoom=(v.zoom/1.2).max(0.25)),
                        Key::Escape=>{hovered.set(None);e.prevent_default();e.stop_propagation();return;},_=>return,
                    }
                    e.prevent_default();e.stop_propagation();active.set(at);hovered.set(Some(at));
                },
                defs {clipPath {id:"{clip}",rect {x:"100",y:"50",width:"720",height:"360"}}}
                line {class:"mk-plot-axis",x1:"100",x2:"820",y1:"410",y2:"410"}
                line {class:"mk-plot-axis",x1:"100",x2:"100",y1:"50",y2:"410"}
                for (tick,label) in x_ticks {
                    {let x=100.0+W*xa.unit(tick);rsx!{
                        text {class:"mk-plot-tick",style:"font-size:{tick_font}px",x:"{x}",y:"432",text_anchor:"middle","{label}"}
                    }}
                }
                for tick in ya.ticks(H*svg_scale,50.0) { {let y=410.0-H*ya.unit(tick);rsx!{text {class:"mk-plot-tick",style:"font-size:{tick_font}px",x:"90",y:"{y}",text_anchor:"end",{ys.label(tick)}}}} }
                text {class:"mk-plot-label",style:"font-size:{label_font}px",x:"460",y:"475",text_anchor:"middle",{x_label.chars().take(x_label_cap).collect::<String>()}}
                text {class:"mk-plot-label",style:"font-size:{label_font}px",transform:"translate(20,230) rotate(-90)",text_anchor:"middle",{y_label.chars().take(y_label_cap).collect::<String>()}}
                g {clip_path:"url(#{clip})",
                    if s.mark==Mark::Line {polyline {class:"mk-plot-line",points:polyline}}
                    for (i,p) in points.iter().enumerate() {
                        if let Some((x,y))=coords[i] {
                            {let ids=selections[i].clone();let panel=panel.clone();let row=p.row;let is_selected=highlighted[i];let is_active=hovered()==Some(i);let radius=if is_selected||is_active {6.0/svg_scale} else {4.0/svg_scale};let label=tooltip(&data.0,row);let class=if is_selected {"mk-plot-point mk-plot-selected"} else if is_active {"mk-plot-point mk-plot-active"} else {"mk-plot-point"};rsx!{
                                g {class,"data-row":"{row}","data-selected":"{is_selected}",role:"button","aria-label":"{label}","aria-pressed":"{is_selected}",
                                    onpointerenter:move |_|hovered.set(Some(i)),onclick:move |e|{e.stop_propagation();if !*moved.peek() {active.set(i);ws.select_linked(&panel,ids.clone());}},
                                    title {"{label}"}
                                    if matches!(s.mark,Mark::Bar|Mark::Histogram) {
                                        {let width=(W*prepared.bar_width/(xa.hi-xa.lo)).clamp(1.0,1000.0);let base=if ys==Scale::Linear {(410.0-H*ya.unit(0.0)).clamp(50.0,410.0)} else {410.0};rsx!{rect {x:"{x-width/2.0}",y:"{y.min(base)}",width:"{width}",height:"{(base-y).abs().max(1.0)}"}}}
                                    } else {circle {cx:"{x}",cy:"{y}",r:"{radius}"}}
                                }
                            }}
                        }
                    }
                }
            }
            if points.is_empty() {p {class:"mk-plot-empty",{t!(ws,L,"plot-no-values")}}}
            pre {class:"mk-plot-tooltip",role:"status","{info}"}
        }
    }
}
fn mark_name(ws: Workspace, m: Mark) -> String {
    match m {
        Mark::Line => t!(ws, L, "plot-line"),
        Mark::Scatter => t!(ws, L, "plot-scatter"),
        Mark::Bar => t!(ws, L, "plot-bar"),
        Mark::Histogram => t!(ws, L, "plot-histogram"),
    }
}
fn scale_name(ws: Workspace, s: Scale) -> String {
    match s {
        Scale::Linear => t!(ws, L, "plot-linear"),
        Scale::Log => t!(ws, L, "plot-log"),
        Scale::Time => t!(ws, L, "plot-time"),
    }
}
