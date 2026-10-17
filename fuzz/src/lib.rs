//! Exercise production source directly, without compiling the Dioxus UI.
//! Source paths deliberately keep fuzzing tied to the parser that ships.
#[allow(dead_code)]
#[path = "../../packages/ext-api/src/flow.rs"]
pub mod flow;
#[allow(dead_code)]
#[path = "../../packages/code-view/src/native_math_parser.rs"]
mod math;

/// Assert valid, ordered UTF-8 ranges and the renderer's formula size bound.
pub fn math_delimiters(source: &str) {
    let mut end = 0;
    for (range, formula) in math::displays(source) {
        assert!(range.start >= end);
        assert!(range.start <= range.end);
        assert!(source.get(range.clone()).is_some());
        assert!(formula.tex.len() <= math::MAX_TEX);
        assert!(formula.display);
        end = range.end;
    }
}

/// Successful parse/serialize/parse must reach a stable representation.
pub fn flow_format(source: &str) {
    if let Ok(flow) = flow::Flow::parse(source) {
        let text = flow.to_json();
        let reparsed = flow::Flow::parse(&text).expect("serialized flow must parse");
        // JSON floating-point parsing can round once; compare stable JSON after
        // reparsing rather than requiring bit identity to the original float.
        assert_eq!(text, reparsed.to_json());
    }
}
