//! The block kinds. Shapes are `[features]` for vectors and
//! `[width, height, channels]` for images (Lux's WHCN order, batch omitted).

use moonkale_ext_api::flow::{BlockKind, FlowLibrary, Param, ParamKind, Port, PortType};

fn port(name: &str, ty: PortType, required: bool) -> Port {
    Port {
        name: name.into(),
        ty,
        required,
    }
}
fn p(name: &str, kind: ParamKind, default: &str) -> Param {
    Param {
        name: name.into(),
        kind,
        default: default.into(),
    }
}
fn choice(options: &[&str]) -> ParamKind {
    ParamKind::Choice {
        options: options.iter().map(|s| s.to_string()).collect(),
    }
}
fn block(id: &str, name: &str, category: &str, description: &str, inputs: Vec<Port>, outputs: Vec<Port>, params: Vec<Param>) -> BlockKind {
    BlockKind {
        id: id.into(),
        name: name.into(),
        category: category.into(),
        description: description.into(),
        inputs,
        outputs,
        params,
    }
}

/// Any-rank tensor.
fn tensor() -> PortType {
    PortType::tensor(Vec::new())
}
fn vector() -> PortType {
    PortType::tensor(vec![None])
}
fn image() -> PortType {
    PortType::tensor(vec![None, None, None])
}

pub fn library() -> FlowLibrary {
    let activations = ["identity", "relu", "gelu", "tanh", "sigmoid", "softmax"];
    FlowLibrary {
        id: "lux",
        name: "Lux.jl",
        language: "julia",
        codegen: Some(crate::codegen::generate),
        blocks: vec![
            block("input", "Input", "data", "The model input: a vector (`features`) or an image (`width×height×channels`).",
                vec![], vec![port("out", tensor(), false)],
                vec![p("shape", ParamKind::Text, "28,28,1"), p("batch", ParamKind::Int, "32")]),
            block("dense", "Dense", "layers", "Fully connected layer `Dense(in => out, activation)`.",
                vec![port("in", vector(), true)], vec![port("out", vector(), false)],
                vec![p("out", ParamKind::Int, "128"), p("activation", choice(&activations), "relu")]),
            block("conv", "Conv", "layers", "2-D convolution `Conv((k, k), in => out, activation; pad)`.",
                vec![port("in", image(), true)], vec![port("out", image(), false)],
                vec![p("channels", ParamKind::Int, "16"), p("kernel", ParamKind::Int, "3"), p("pad", ParamKind::Int, "1"), p("activation", choice(&activations), "relu")]),
            block("maxpool", "MaxPool", "layers", "`MaxPool((k, k))`.",
                vec![port("in", image(), true)], vec![port("out", image(), false)],
                vec![p("kernel", ParamKind::Int, "2")]),
            block("flatten", "Flatten", "layers", "`FlattenLayer()`: image → vector.",
                vec![port("in", image(), true)], vec![port("out", vector(), false)], vec![]),
            block("dropout", "Dropout", "layers", "`Dropout(p)`.",
                vec![port("in", tensor(), true)], vec![port("out", tensor(), false)],
                vec![p("p", ParamKind::Float, "0.5")]),
            block("batchnorm", "BatchNorm", "layers", "`BatchNorm(channels)` (channels are inferred from the incoming block).",
                vec![port("in", tensor(), true)], vec![port("out", tensor(), false)], vec![]),
            block("loss", "Loss", "training", "Loss between the model output and the labels.",
                vec![port("prediction", vector(), true)], vec![port("loss", PortType::Named { name: "loss".into() }, false)],
                vec![p("kind", choice(&["CrossEntropyLoss", "MSELoss", "MAELoss"]), "CrossEntropyLoss"), p("classes", ParamKind::Int, "10")]),
            block("optimiser", "Optimiser", "training", "Optimisers.jl rule and the training loop.",
                vec![port("loss", PortType::Named { name: "loss".into() }, true)], vec![],
                vec![p("rule", choice(&["Adam", "SGD", "AdamW"]), "Adam"), p("lr", ParamKind::Float, "0.001"), p("epochs", ParamKind::Int, "5")]),
        ],
    }
}
