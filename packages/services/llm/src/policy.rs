//! The gate. Every tool call is classified and a decision made before the
//! host runs it. Defaults: reads run; writes ask; destructive statements
//! always ask (and are refused outright when the source is read-only).

use crate::tools::ToolCall;
use moonkale_core::source::risk;
use moonkale_core::Risk;
use serde::{Deserialize, Serialize};

pub use moonkale_llm_types::{Class, Decision};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Writes: `Ask` (default) or `Allow` (trusted session) or `Deny`.
    pub mutating: Decision,
    /// Destructive statements: `Ask` (default) or `Deny`.
    pub destructive: Decision,
    /// Tool names the agent may never call.
    pub denied_tools: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            mutating: Decision::Ask,
            destructive: Decision::Ask,
            denied_tools: Vec::new(),
        }
    }
}

impl Policy {
    /// The class of a call, with text queries classified by the shared
    /// rules ([`risk::classify`]). Prefer [`Policy::classify_with`] and the
    /// source's own answer when the source is at hand.
    pub fn classify(call: &ToolCall) -> Class {
        Self::classify_with(call, None)
    }

    /// The class of a call; `text_risk` is what the target source said about
    /// a `source.text_query` (`Source::classify`), `None` = the shared rules.
    pub fn classify_with(call: &ToolCall, text_risk: Option<Risk>) -> Class {
        match call.name.as_str() {
            "source.text_query" => {
                let risk = text_risk.unwrap_or_else(|| {
                    risk::classify(
                        call.str("dialect").unwrap_or("sql"),
                        call.str("text").unwrap_or_default(),
                    )
                });
                match risk {
                    Risk::Read => Class::ReadOnly,
                    Risk::Write | Risk::Unknown => Class::Mutating,
                    Risk::Destructive => Class::Destructive,
                }
            }
            "editor.replace" | "file.create" => Class::Mutating,
            "terminal.run" => classify_command(call.str("command").unwrap_or_default()),
            // The built-in read tools.
            "workspace.list_sources"
            | "graph.query"
            | "graph.fetch"
            | "index.search"
            | "editor.open" => Class::ReadOnly,
            // Anything else is third-party (wasm) code: ask.
            _ => Class::Mutating,
        }
    }

    pub fn decide(&self, call: &ToolCall) -> (Class, Decision) {
        self.decide_with(call, None)
    }

    /// [`Policy::decide`] with the target source's classification of a text
    /// query (see [`Policy::classify_with`]).
    pub fn decide_with(&self, call: &ToolCall, text_risk: Option<Risk>) -> (Class, Decision) {
        if self.denied_tools.iter().any(|t| t == &call.name) {
            return (Self::classify_with(call, text_risk), Decision::Deny);
        }
        let class = Self::classify_with(call, text_risk);
        let decision = match class {
            Class::ReadOnly => Decision::Allow,
            Class::Mutating => self.mutating,
            Class::Destructive => match self.destructive {
                Decision::Allow => Decision::Ask, // never silently
                d => d,
            },
        };
        (class, decision)
    }
}

/// Shell commands: obviously destructive ones ask every time; the rest are
/// mutating (they run code). A denylist can never be complete — it decides
/// only whether a command asks *every* time — but it reads the command as
/// words, per simple command, so spacing, case, a path to the binary or a
/// wrapper (`sudo`, `xargs`, `env`, …) does not hide `rm -r` (#9).
pub fn classify_command(cmd: &str) -> Class {
    let c = cmd.to_ascii_lowercase();
    // Patterns that are destructive wherever they appear.
    if [":(){", "drop table", "drop database", "truncate table"]
        .iter()
        .any(|p| c.contains(p))
    {
        return Class::Destructive;
    }
    let simple = c.split([';', '&', '|', '\n', '(', ')', '`']);
    if simple
        .into_iter()
        .any(|part| destructive_simple(part.split_whitespace().collect()))
    {
        Class::Destructive
    } else {
        Class::Mutating
    }
}

fn destructive_simple(mut words: Vec<&str>) -> bool {
    // Writing to a raw device (`> /dev/sda`; `/dev/null` is harmless).
    if words
        .windows(2)
        .any(|w| w[0].ends_with('>') && w[1].starts_with("/dev/") && w[1] != "/dev/null")
        || words
            .iter()
            .any(|w| w.contains(">/dev/") && !w.ends_with(">/dev/null"))
    {
        return true;
    }
    // Strip environment assignments and wrappers to reach the program.
    loop {
        let Some(first) = words.first() else {
            return false;
        };
        let prog = first.rsplit('/').next().unwrap_or(first);
        if first.contains('=') && !first.starts_with('-') {
            words.remove(0);
        } else if matches!(prog, "sudo" | "doas" | "su" | "pkexec") {
            return true;
        } else if matches!(
            prog,
            "env"
                | "nice"
                | "nohup"
                | "time"
                | "exec"
                | "command"
                | "builtin"
                | "xargs"
                | "timeout"
        ) {
            words.remove(0);
            // Their own flags and arguments (`timeout 5`, `xargs -0`).
            while words
                .first()
                .is_some_and(|w| w.starts_with('-') || w.parse::<f64>().is_ok())
            {
                words.remove(0);
            }
        } else {
            break;
        }
    }
    let prog = words[0].rsplit('/').next().unwrap_or(words[0]);
    let args = &words[1..];
    let flag = |short: char, long: &str| {
        args.iter().any(|a| {
            *a == long || (a.starts_with('-') && !a.starts_with("--") && a[1..].contains(short))
        })
    };
    match prog {
        "rm" => flag('r', "--recursive"),
        "rmdir" | "shred" | "wipefs" | "fdisk" | "sfdisk" | "parted" | "dd" | "truncate"
        | "shutdown" | "reboot" | "poweroff" | "halt" => true,
        p if p.starts_with("mkfs") => true,
        "find" => {
            args.contains(&"-delete")
                || args.windows(2).any(|w| {
                    matches!(w[0], "-exec" | "-execdir" | "-ok" | "-okdir")
                        && matches!(
                            w[1].rsplit('/').next().unwrap_or(""),
                            "rm" | "shred" | "unlink"
                        )
                })
        }
        "chmod" | "chown" | "chgrp" => flag('r', "--recursive"),
        "git" => {
            let sub = args
                .iter()
                .find(|a| !a.starts_with('-'))
                .copied()
                .unwrap_or("");
            match sub {
                "push" => {
                    flag('f', "--force")
                        || args
                            .iter()
                            .any(|a| a.starts_with("--force-with-lease") || a.starts_with('+'))
                        || args.contains(&"--delete")
                        || args.contains(&"--mirror")
                }
                "reset" => args.contains(&"--hard"),
                "clean" => flag('f', "--force"),
                "branch" => args
                    .iter()
                    .any(|a| *a == "-d" || *a == "-D" || *a == "--delete"),
                "checkout" | "restore" => args.contains(&".") || args.contains(&"--"),
                "filter-branch" | "filter-repo" => true,
                _ => false,
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn commands_are_read_as_words() {
        for c in [
            "rm -rf /",
            "rm  -rf  build",
            "RM -Rf x",
            "/bin/rm -r x",
            "rm --recursive x",
            "find / -delete",
            "find . -name '*.o' -exec rm {} +",
            "ls | xargs rm -r",
            "ls | xargs -0 rm -fr",
            "FOO=1 rm -r x",
            "sudo ls",
            "git push --force-with-lease",
            "git push origin +main",
            "git clean -xfd",
            "git reset --hard HEAD~1",
            "echo hi > /dev/sda",
            "mkfs.ext4 /dev/sdb1",
            "cd x && rm -r y",
            "psql -c 'DROP TABLE t'",
        ] {
            assert_eq!(classify_command(c), Class::Destructive, "{c}");
        }
        for c in [
            "rm file.txt",
            "cargo build 2>/dev/null",
            "echo hi > /dev/null",
            "git push origin main",
            "git status",
            "find . -name '*.rs'",
            "ls -la",
        ] {
            assert_eq!(classify_command(c), Class::Mutating, "{c}");
        }
    }

    fn call(dialect: &str, text: &str) -> ToolCall {
        ToolCall {
            id: "1".into(),
            name: "source.text_query".into(),
            input: json!({ "source": "s", "dialect": dialect, "text": text }),
        }
    }

    #[test]
    fn sql_and_cypher_classes() {
        let p = Policy::default();
        assert_eq!(
            p.decide(&call("sql", "SELECT * FROM t")),
            (Class::ReadOnly, Decision::Allow)
        );
        assert_eq!(
            p.decide(&call("sql", "UPDATE t SET a = 1")),
            (Class::Mutating, Decision::Ask)
        );
        assert_eq!(
            p.decide(&call("sql", "DROP TABLE t")),
            (Class::Destructive, Decision::Ask)
        );
        assert_eq!(
            p.decide(&call("cypher", "MATCH (n) RETURN n")),
            (Class::ReadOnly, Decision::Allow)
        );
        assert_eq!(
            p.decide(&call("cypher", "CREATE (:Person {name:'x'})")),
            (Class::Mutating, Decision::Ask)
        );
        assert_eq!(
            p.decide(&call("cypher", "MATCH (n) DETACH DELETE n")),
            (Class::Destructive, Decision::Ask)
        );
    }

    #[test]
    fn write_tools_and_commands() {
        let p = Policy::default();
        let rep = ToolCall {
            id: "3".into(),
            name: "editor.replace".into(),
            input: json!({"old":"a","new":"b"}),
        };
        assert_eq!(p.decide(&rep), (Class::Mutating, Decision::Ask));
        let run = |c: &str| ToolCall {
            id: "4".into(),
            name: "terminal.run".into(),
            input: json!({"command": c}),
        };
        assert_eq!(
            p.decide(&run("cargo test")),
            (Class::Mutating, Decision::Ask)
        );
        assert_eq!(
            p.decide(&run("rm -rf target")),
            (Class::Destructive, Decision::Ask)
        );
        let trusting = Policy {
            mutating: Decision::Allow,
            ..Default::default()
        };
        assert_eq!(trusting.decide(&run("cargo test")).1, Decision::Allow);
        assert_eq!(trusting.decide(&run("git push --force")).1, Decision::Ask);
    }

    #[test]
    fn denied_and_never_silent_destructive() {
        let p = Policy {
            destructive: Decision::Allow,
            denied_tools: vec!["editor.open".into()],
            ..Default::default()
        };
        assert_eq!(p.decide(&call("sql", "DROP TABLE t")).1, Decision::Ask);
        let open = ToolCall {
            id: "2".into(),
            name: "editor.open".into(),
            input: json!({}),
        };
        assert_eq!(p.decide(&open).1, Decision::Deny);
    }

    #[test]
    fn the_sources_own_classification_wins() {
        let p = Policy::default();
        // A source that knows `SELECT read_csv(...)` reads files may call it a write.
        let c = call("sql", "SELECT * FROM read_csv('/etc/passwd')");
        assert_eq!(p.decide(&c).0, Class::ReadOnly);
        assert_eq!(
            p.decide_with(&c, Some(Risk::Write)),
            (Class::Mutating, Decision::Ask)
        );
    }
}
