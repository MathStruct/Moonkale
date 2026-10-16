#![cfg(feature = "lossless-syntax")]
use moonkale_index::lossless::{
    RustSyntaxSnapshot as Snapshot, SyntaxEditError as Error, MAX_SOURCE_BYTES,
};

#[test]
fn lossless_corpus_preserves_every_byte_and_partitions_tokens_and_trivia() {
    for source in [
        "",
        " \r\n// only trivia\r\n",
        "\u{feff}fn f(){}",
        "#!/usr/bin/env moonkale\n#![allow(dead_code)]\nfn f() {}\n",
        "/// Docs λ\r\n#[inline]\r\npub fn café(x: &str) { /* α */ println!(\"{x}\"); } // EOF",
        "macro_rules! m { ($x:expr) => { $x }; }\nfn f(){m!(1);}\n",
        "// first\nfn one() { /* block */ }\n\n// next\nfn two() {}\n// tail\n",
        include_str!("../../../core/src/source/descriptor.rs"),
        include_str!("../src/lossless.rs"),
    ] {
        let snapshot = Snapshot::parse(source).unwrap_or_else(|e| panic!("{e}: {source}"));
        assert_eq!(snapshot.print_original().as_bytes(), source.as_bytes());
        let mut end = 0;
        for token in snapshot.tokens() {
            assert_eq!(token.leading_trivia.start, end);
            assert_eq!(token.leading_trivia.end, token.range.start);
            assert!(source.get(token.range.clone()).is_some());
            end = token.range.end;
        }
        assert_eq!(snapshot.eof_trivia(), end..source.len());
    }
}

#[test]
fn declaration_rename_changes_only_the_name_token_in_unicode_crlf_source() {
    let source =
        "// 🦀\r\n/// Docs\r\n#[inline]\r\npub fn café() { café(); let s = \"café\"; } // tail\r\n";
    let snapshot = Snapshot::parse(source).unwrap();
    let plan = snapshot.plan_declaration_rename(0, "r#type").unwrap();
    assert_eq!(plan.edits().len(), 1);
    let edit = &plan.edits()[0];
    let next = plan.apply_to(source).unwrap();
    assert_eq!(&next[..edit.range.start], &source[..edit.range.start]);
    assert_eq!(
        &next[edit.range.start + edit.inserted_text.len()..],
        &source[edit.range.end..]
    );
    assert!(next.contains("r#type() { café(); let s = \"café\""));
    assert_eq!(
        plan.apply_to(&format!("{source}// external")),
        Err(Error::StaleInput)
    );
}

#[test]
fn function_moves_carry_leading_docs_and_trailing_comments_without_reformatting() {
    let source = "// A\r\nfn a() {} // trailing A\r\n\r\n/// B\r\n#[inline]\r\nfn b() { let x = 1; }\r\n// EOF owned by file\r\n";
    let snapshot = Snapshot::parse(source).unwrap();
    let a = snapshot.functions()[0].owned_range.clone();
    let b = snapshot.functions()[1].owned_range.clone();
    let plan = snapshot.plan_move_function(1, Some(0)).unwrap();
    let next = plan.apply_to(source).unwrap();
    let expected = format!("{}{}{}", &source[b.clone()], &source[a], &source[b.end..]);
    assert_eq!(next, expected);
    assert!(next.ends_with("// EOF owned by file\r\n"));
    let snapshot2 = Snapshot::parse(&next).unwrap();
    assert_eq!(snapshot2.functions()[0].name, "b");
    assert_eq!(
        snapshot
            .plan_move_function(0, None)
            .unwrap()
            .apply_to(source)
            .unwrap(),
        next
    );
    assert!(snapshot
        .plan_move_function(0, Some(0))
        .unwrap()
        .edits()
        .is_empty());
    assert!(snapshot
        .plan_move_function(1, None)
        .unwrap()
        .edits()
        .is_empty());
}

#[test]
fn protected_file_prefix_and_non_function_items_stay_put() {
    for source in [
        "\u{feff}#![allow(dead_code)]\nfn a(){}\nfn b(){}\n",
        "#!/usr/bin/env moonkale\n#![allow(dead_code)]\nfn a(){}\nfn b(){}\n",
        "const HEADER:i32 = 1;\nfn a(){}\nfn b(){}\n",
    ] {
        let snapshot = Snapshot::parse(source).unwrap();
        let prefix = &source[..snapshot.functions()[0].owned_range.start];
        let next = snapshot
            .plan_move_function(1, Some(0))
            .unwrap()
            .apply_to(source)
            .unwrap();
        assert!(next.starts_with(prefix));
        assert_eq!(
            Snapshot::parse(&next)
                .unwrap()
                .functions()
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
    }
}

#[test]
fn invalid_names_functions_and_parser_resources_are_rejected() {
    let snapshot = Snapshot::parse("fn f() {} fn g() {} // no newline").unwrap();
    for name in [
        "",
        "x(){} fn injected",
        "foo /* comment */",
        "a b",
        "self",
        "r#self",
        "fn",
    ] {
        assert_eq!(
            snapshot.plan_declaration_rename(0, name),
            Err(Error::InvalidName),
            "{name}"
        );
    }
    assert_eq!(
        snapshot.plan_declaration_rename(99, "valid"),
        Err(Error::InvalidFunction)
    );
    assert_eq!(
        snapshot.plan_move_function(99, None),
        Err(Error::InvalidFunction)
    );
    assert_eq!(
        snapshot.plan_move_function(0, Some(99)),
        Err(Error::InvalidFunction)
    );
    assert!(snapshot
        .plan_declaration_rename(0, "f")
        .unwrap()
        .edits()
        .is_empty());
    assert!(matches!(
        Snapshot::parse("fn broken( {"),
        Err(Error::MalformedSyntax)
    ));
    assert!(matches!(
        Snapshot::parse(&" ".repeat(MAX_SOURCE_BYTES + 1)),
        Err(Error::LimitExceeded)
    ));
    let deep = format!("fn f() {{ {}1{}; }}", "(".repeat(140), ")".repeat(140));
    assert!(matches!(Snapshot::parse(&deep), Err(Error::LimitExceeded)));
}

#[test]
fn eof_line_comments_cannot_swallow_other_items_during_a_move() {
    for source in [
        "fn a(){}\nfn b(){} // no newline",
        "fn a(){} const X:i32=1;\nfn b(){} // EOF",
    ] {
        let snapshot = Snapshot::parse(source).unwrap();
        assert_eq!(
            snapshot.plan_move_function(1, Some(0)),
            Err(Error::InvalidPlacement)
        );
        assert_eq!(snapshot.print_original(), source);
    }
    let source = "fn a(){} /* trailing block */\r\nfn b(){}\r\n";
    let snapshot = Snapshot::parse(source).unwrap();
    assert!(source[snapshot.functions()[0].owned_range.clone()].ends_with("*/\r\n"));
    assert!(snapshot.plan_move_function(1, Some(0)).is_ok());
}

#[test]
fn whitespace_is_only_trivia_and_contextual_names_remain_available() {
    let snapshot = Snapshot::parse(" \t\r\n ").unwrap();
    assert!(snapshot.tokens().is_empty());
    assert_eq!(snapshot.eof_trivia(), 0..5);
    let snapshot = Snapshot::parse("fn f(){}").unwrap();
    for name in ["_", "r#_", "gen", "async", "r#crate"] {
        assert_eq!(
            snapshot.plan_declaration_rename(0, name),
            Err(Error::InvalidName)
        );
    }
    for name in ["r#async", "union", "_ok", "東京"] {
        assert!(snapshot.plan_declaration_rename(0, name).is_ok(), "{name}");
    }
}

#[test]
fn output_limit_counts_the_final_size_not_intermediate_move_order() {
    let prefix = "fn a(){}\nfn b(){}\n// ";
    let source = format!("{prefix}{}", "x".repeat(MAX_SOURCE_BYTES - prefix.len()));
    let snapshot = Snapshot::parse(&source).unwrap();
    let moved = snapshot
        .plan_move_function(1, Some(0))
        .unwrap()
        .apply_to(&source)
        .unwrap();
    assert_eq!(moved.len(), source.len());
    assert_eq!(
        snapshot.plan_declaration_rename(0, "longer_name"),
        Err(Error::LimitExceeded)
    );
}
