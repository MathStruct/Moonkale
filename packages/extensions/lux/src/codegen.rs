//! Flow → `model.jl`. The layer chain is the path from `Input` to `Loss`
//! (blocks in topological order along the wires); everything else is
//! reported as an error so a half-wired flow never produces a silently wrong
//! file.

use moonkale_ext_api::flow::{Block, Flow, FlowLibrary, Generated};

fn param<'a>(b: &'a Block, name: &str, default: &'a str) -> &'a str {
    b.params.get(name).map(|s| s.as_str()).unwrap_or(default)
}

/// Julia integer/float literal from a parameter, validated.
fn num(b: &Block, name: &str, default: &str) -> Result<String, String> {
    let v = param(b, name, default).trim();
    v.parse::<f64>()
        .map(|_| v.to_string())
        .map_err(|_| format!("{}.{name} = {v:?} is not a number", b.id))
}

pub fn generate(flow: &Flow, lib: &FlowLibrary) -> Result<Generated, String> {
    let prefix = format!("{}/", lib.id);
    let kind = |b: &Block| b.kind.strip_prefix(&prefix).unwrap_or(&b.kind).to_string();
    let order = flow.topological()?;
    let input = order
        .iter()
        .find(|b| kind(b) == "input")
        .ok_or("the flow needs an Input block")?;
    // Follow the single output wire from block to block.
    let next = |from: &Block| -> Option<&Block> {
        flow.wires
            .iter()
            .find(|w| w.from_block == from.id)
            .and_then(|w| flow.block(&w.to_block))
    };
    let mut layers: Vec<String> = Vec::new();
    let mut cur = next(input);
    let mut loss: Option<&Block> = None;
    let mut optimiser: Option<&Block> = None;
    let mut in_features: Option<String> = shape_features(param(input, "shape", "28,28,1"));
    let mut in_channels: Option<String> = shape_channels(param(input, "shape", "28,28,1"));
    let mut guard = 0;
    while let Some(b) = cur {
        guard += 1;
        if guard > 256 {
            return Err("chain too long (cycle?)".into());
        }
        match kind(b).as_str() {
            "dense" => {
                let out = num(b, "out", "128")?;
                let act = param(b, "activation", "relu");
                let inp = in_features.clone().ok_or_else(|| format!("{}: input size unknown — put a Flatten before a Dense after image layers", b.id))?;
                layers.push(format!("Dense({inp} => {out}, {act})"));
                in_features = Some(out);
            }
            "conv" => {
                let ch = num(b, "channels", "16")?;
                let k = num(b, "kernel", "3")?;
                let pad = num(b, "pad", "1")?;
                let act = param(b, "activation", "relu");
                let inp = in_channels.clone().ok_or_else(|| format!("{}: channel count unknown", b.id))?;
                layers.push(format!("Conv(({k}, {k}), {inp} => {ch}, {act}; pad={pad})"));
                in_channels = Some(ch);
                in_features = None;
            }
            "maxpool" => {
                let k = num(b, "kernel", "2")?;
                layers.push(format!("MaxPool(({k}, {k}))"));
            }
            "flatten" => {
                layers.push("FlattenLayer()".into());
                // Features after flatten depend on the spatial size; let Lux
                // infer them lazily.
                in_features = Some("Lux.NoInputSize".into());
            }
            "dropout" => layers.push(format!("Dropout({})", num(b, "p", "0.5")?)),
            "batchnorm" => {
                let ch = in_channels.clone().or(in_features.clone()).ok_or("BatchNorm: size unknown")?;
                layers.push(format!("BatchNorm({ch})"));
            }
            "loss" => {
                loss = Some(b);
                cur = next(b);
                if let Some(o) = cur {
                    if kind(o) == "optimiser" {
                        optimiser = Some(o);
                    }
                }
                break;
            }
            other => return Err(format!("{}: block kind {other} cannot be part of the chain", b.id)),
        }
        cur = next(b);
    }
    let loss = loss.ok_or("the chain must end in a Loss block")?;
    if layers.is_empty() {
        return Err("the flow has no layers between Input and Loss".into());
    }
    // `Lux.NoInputSize` is a placeholder for "use a lazy Dense": Lux needs the
    // size, so we compute it by running the front of the chain once.
    let lazy = layers.iter().any(|l| l.contains("Lux.NoInputSize"));
    let chain = layers
        .iter()
        .map(|l| l.replace("Lux.NoInputSize", "flat_features"))
        .collect::<Vec<_>>()
        .join(",\n    ");
    let shape = param(input, "shape", "28,28,1")
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let batch = num(input, "batch", "32")?;
    let loss_kind = param(loss, "kind", "CrossEntropyLoss");
    let classes = num(loss, "classes", "10")?;
    let (rule, lr, epochs) = match optimiser {
        Some(o) => (
            param(o, "rule", "Adam").to_string(),
            num(o, "lr", "0.001")?,
            num(o, "epochs", "5")?,
        ),
        None => ("Adam".into(), "0.001".into(), "5".into()),
    };
    let mut text = String::new();
    text.push_str("# Generated by Moonkale's Lux.jl extension from the flow editor.\n");
    text.push_str("# Needs: using Pkg; Pkg.add([\"Lux\", \"Optimisers\", \"Zygote\"])\n");
    text.push_str("using Lux, Optimisers, Zygote, Random\n\n");
    text.push_str(&format!("const INPUT_SHAPE = ({shape})\nconst BATCH = {batch}\nconst CLASSES = {classes}\n\n"));
    if lazy {
        text.push_str("# Features after FlattenLayer, computed once from the input shape.\n");
        let front: Vec<&String> = layers.iter().take_while(|l| !l.contains("Lux.NoInputSize")).collect();
        text.push_str("const flat_features = let\n    front = Chain(\n        ");
        text.push_str(&front.iter().map(|l| l.as_str()).collect::<Vec<_>>().join(",\n        "));
        text.push_str("\n    )\n    ps, st = Lux.setup(Random.default_rng(), front)\n    y, _ = front(randn(Float32, INPUT_SHAPE..., 1), ps, st)\n    prod(size(y)[1:end-1])\nend\n\n");
    }
    text.push_str(&format!("model = Chain(\n    {chain}\n)\n\n"));
    text.push_str("rng = Random.default_rng()\nps, st = Lux.setup(rng, model)\n\n");
    text.push_str(&format!("# Replace with real data: x is INPUT_SHAPE × BATCH, y is one-hot CLASSES × BATCH.\nx = randn(Float32, INPUT_SHAPE..., BATCH)\ny = Lux.onehotbatch(rand(rng, 1:CLASSES, BATCH), 1:CLASSES)\n\n"));
    text.push_str(&format!("loss_fn = {loss_kind}()\nopt = Optimisers.{rule}({lr})\ntrain_state = Lux.Training.TrainState(model, ps, st, opt)\n\n"));
    text.push_str(&format!("for epoch in 1:{epochs}\n    _, loss, _, train_state = Lux.Training.single_train_step!(AutoZygote(), loss_fn, (x, y), train_state)\n    println(\"epoch \", epoch, \" loss \", loss)\nend\n"));
    Ok(Generated {
        file_name: "model.jl".into(),
        text,
        run_hint: "julia model.jl".into(),
    })
}

fn shape_dims(shape: &str) -> Vec<String> {
    shape
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
fn shape_features(shape: &str) -> Option<String> {
    let d = shape_dims(shape);
    (d.len() == 1).then(|| d[0].clone())
}
fn shape_channels(shape: &str) -> Option<String> {
    let d = shape_dims(shape);
    (d.len() == 3).then(|| d[2].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_ext_api::flow::{Block, Wire};
    use std::collections::BTreeMap;

    fn b(id: &str, kind: &str, params: &[(&str, &str)]) -> Block {
        Block {
            id: id.into(),
            kind: format!("lux/{kind}"),
            x: 0.0,
            y: 0.0,
            params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
        }
    }
    fn w(a: &str, ap: &str, b: &str, bp: &str) -> Wire {
        Wire { id: format!("{a}-{b}"), from_block: a.into(), from_port: ap.into(), to_block: b.into(), to_port: bp.into() }
    }

    #[test]
    fn cnn_generates_a_chain_with_lazy_flatten_size() {
        let lib = crate::library();
        let mut f = Flow::new();
        f.blocks = vec![
            b("in", "input", &[("shape", "28,28,1")]),
            b("c1", "conv", &[("channels", "8"), ("kernel", "3")]),
            b("p1", "maxpool", &[]),
            b("fl", "flatten", &[]),
            b("d1", "dense", &[("out", "10"), ("activation", "identity")]),
            b("l", "loss", &[]),
            b("o", "optimiser", &[("epochs", "2")]),
        ];
        f.wires = vec![w("in", "out", "c1", "in"), w("c1", "out", "p1", "in"), w("p1", "out", "fl", "in"), w("fl", "out", "d1", "in"), w("d1", "out", "l", "prediction"), w("l", "loss", "o", "loss")];
        assert!(moonkale_ext_api::flow::validate(&f, &[lib.clone()]).is_empty());
        let g = generate(&f, &lib).unwrap();
        assert_eq!(g.file_name, "model.jl");
        assert!(g.text.contains("Conv((3, 3), 1 => 8, relu; pad=1)"), "{}", g.text);
        assert!(g.text.contains("Dense(flat_features => 10, identity)"));
        assert!(g.text.contains("const flat_features = let"));
        assert!(g.text.contains("for epoch in 1:2"));
    }

    #[test]
    fn errors_are_specific() {
        let lib = crate::library();
        let mut f = Flow::new();
        f.blocks = vec![b("in", "input", &[("shape", "28,28,1")]), b("d", "dense", &[]), b("l", "loss", &[])];
        f.wires = vec![w("in", "out", "d", "in"), w("d", "out", "l", "prediction")];
        let e = generate(&f, &lib).unwrap_err();
        assert!(e.contains("Flatten"), "{e}");
        f.blocks[1].params.insert("out".into(), "many".into());
        f.blocks[0].params.insert("shape".into(), "784".into());
        assert!(generate(&f, &lib).unwrap_err().contains("not a number"));
    }
}
