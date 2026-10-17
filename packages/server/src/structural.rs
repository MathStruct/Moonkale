//! Pure structural planning, shared by desktop workers and the web endpoint.
use dioxus::prelude::*;
pub use moonkale_ext_api::structural::StructuralPlanner;
use moonkale_ext_api::structural::*;

/// Web transport; errors never include file contents.
pub fn remote(request: PlanRequest) -> PlanFuture {
    Box::pin(async move { plan(request).await.map_err(|_| PlanError::Unavailable)? })
}

/// Bounded pure endpoint, inside the ordinary server authentication gate.
#[post("/api/structural/plan")]
pub async fn plan(request: PlanRequest) -> Result<Result<PlanResponse, PlanError>, ServerFnError> {
    Ok(local(request).await)
}

// Avoid dependencies on an executor in the engine-neutral contract.
#[cfg(not(target_arch = "wasm32"))]
/// Schedule a parser outside the input/render thread; never queue unbounded work.
pub fn local(request: PlanRequest) -> PlanFuture {
    Box::pin(async move {
        if request.text.len() > 512 * 1024 {
            return Err(PlanError::Limit);
        }
        let permit = slots()
            .clone()
            .try_acquire_owned()
            .map_err(|_| PlanError::Busy)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            compute(request)
        })
        .await
        .map_err(|_| PlanError::Unavailable)?
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn slots() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    use std::sync::{Arc, OnceLock};
    static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(2)))
}

#[cfg(not(target_arch = "wasm32"))]
fn compute(request: PlanRequest) -> Result<PlanResponse, PlanError> {
    use moonkale_index::lossless::{RustSyntaxSnapshot, SyntaxEditError};
    let error = |e| match e {
        SyntaxEditError::LimitExceeded => PlanError::Limit,
        SyntaxEditError::MalformedSyntax => PlanError::Syntax,
        SyntaxEditError::StaleInput => PlanError::Stale,
        _ => PlanError::Invalid,
    };
    let syntax = RustSyntaxSnapshot::parse(&request.text).map_err(error)?;
    if syntax.functions().len() > 4096 {
        return Err(PlanError::Limit);
    }
    // Walk source once when computing display line numbers.
    let mut previous = 0;
    let mut line = 1;
    let functions: Vec<_> = syntax
        .functions()
        .iter()
        .map(|function| {
            line += request.text.as_bytes()[previous..function.syntax_range.start]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            previous = function.syntax_range.start;
            FunctionHandle {
                input: syntax.input_hash(),
                start: function.syntax_range.start,
                end: function.syntax_range.end,
                name: function.name.clone(),
                line,
            }
        })
        .collect();
    let resolve = |target: &FunctionHandle| {
        functions
            .iter()
            .position(|function| function == target)
            .ok_or(PlanError::Stale)
    };
    let plan = match request.operation {
        Operation::List => None,
        Operation::DeclarationName { target, name } => {
            if name.len() > 256 {
                return Err(PlanError::Limit);
            }
            Some(
                syntax
                    .plan_declaration_rename(resolve(&target)?, &name)
                    .map_err(error)?,
            )
        }
        Operation::Move { target, before } => Some(
            syntax
                .plan_move_function(resolve(&target)?, before.as_ref().map(resolve).transpose()?)
                .map_err(error)?,
        ),
    };
    let edits = if let Some(plan) = plan {
        plan.apply_to(&request.text).map_err(error)?;
        plan.edits()
            .iter()
            .map(|edit| ByteEdit {
                start: edit.range.start,
                end: edit.range.end,
                removed: edit.removed_text.clone(),
                inserted: edit.inserted_text.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    if edits.len() > 4096
        || edits
            .iter()
            .map(|edit: &ByteEdit| edit.removed.len() + edit.inserted.len())
            .sum::<usize>()
            > 1024 * 1024
    {
        return Err(PlanError::Limit);
    }
    Ok(PlanResponse { functions, edits })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    const TEXT: &str = "// 😀\r\nfn old() {}\r\nfn second() {}\r\n";
    fn list(text: &str) -> PlanResponse {
        compute(PlanRequest {
            text: text.into(),
            operation: Operation::List,
        })
        .unwrap()
    }
    #[test]
    fn list_and_unicode_declaration_edits_are_exact_and_serializable() {
        let functions = list(TEXT).functions;
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].line, 2);
        let response = compute(PlanRequest {
            text: TEXT.into(),
            operation: Operation::DeclarationName {
                target: functions[0].clone(),
                name: "新名".into(),
            },
        })
        .unwrap();
        assert_eq!(response.edits.len(), 1);
        assert_eq!(response.edits[0].removed, "old");
        assert_eq!(response.edits[0].inserted, "新名");
        assert_eq!(&TEXT[response.edits[0].start..response.edits[0].end], "old");
        let encoded = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<PlanResponse>(&encoded).unwrap(),
            response
        );
    }
    #[test]
    fn every_handle_is_bound_to_the_entire_exact_input() {
        let target = list(TEXT).functions[0].clone();
        assert_eq!(
            compute(PlanRequest {
                text: format!("{TEXT}// changed"),
                operation: Operation::DeclarationName {
                    target,
                    name: "new".into()
                }
            }),
            Err(PlanError::Stale)
        );
        let functions = list(TEXT).functions;
        let foreign = list("fn other() {}\n").functions[0].clone();
        assert_eq!(
            compute(PlanRequest {
                text: TEXT.into(),
                operation: Operation::Move {
                    target: functions[0].clone(),
                    before: Some(foreign)
                }
            }),
            Err(PlanError::Stale)
        );
    }
    #[test]
    fn move_preserves_trivia_and_rejects_invalid_names_and_syntax() {
        let functions = list(TEXT).functions;
        let moved = compute(PlanRequest {
            text: TEXT.into(),
            operation: Operation::Move {
                target: functions[1].clone(),
                before: Some(functions[0].clone()),
            },
        })
        .unwrap();
        let mut result = TEXT.to_string();
        let mut edits = moved.edits;
        edits.sort_by_key(|edit| (edit.start, edit.end));
        for edit in edits.iter().rev() {
            result.replace_range(edit.start..edit.end, &edit.inserted);
        }
        assert_eq!(result, "\r\nfn second() {}// 😀\r\nfn old() {}\r\n");
        assert!(result.contains("// 😀\r\nfn old() {}\r\n"));
        assert_eq!(
            compute(PlanRequest {
                text: TEXT.into(),
                operation: Operation::DeclarationName {
                    target: functions[0].clone(),
                    name: "fn".into()
                }
            }),
            Err(PlanError::Invalid)
        );
        assert_eq!(
            compute(PlanRequest {
                text: "fn broken(".into(),
                operation: Operation::List
            }),
            Err(PlanError::Syntax)
        );
    }
    #[tokio::test]
    async fn worker_matches_pure_planner_and_rejects_oversized_input() {
        let request = PlanRequest {
            text: TEXT.into(),
            operation: Operation::List,
        };
        let first = slots().clone().try_acquire_owned().unwrap();
        let second = slots().clone().try_acquire_owned().unwrap();
        assert_eq!(local(request.clone()).await, Err(PlanError::Busy));
        drop((first, second));
        assert_eq!(local(request.clone()).await, compute(request));
        assert_eq!(
            local(PlanRequest {
                text: " ".repeat(512 * 1024 + 1),
                operation: Operation::List
            })
            .await,
            Err(PlanError::Limit)
        );
    }
}
