//! `Workspace`: terminals, programs, language servers, Typst, the LLM provider, git. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;
use crate::{t, L};

impl Workspace {
    /// Can this platform run a program in a terminal tab?
    pub fn can_run_program(&self) -> bool {
        self.config.processes.program.is_some()
    }

    /// Start `program args…` under a PTY and hand it to the terminal panel
    /// as a tab titled `title` (Milestone 15: `claude auth login`).
    pub async fn run_in_terminal(mut self, title: &str, program: &str, args: Vec<String>) {
        let Some(spawn) = self.config.processes.program else {
            self.set_status(t!(self, L, "program-unavailable"));
            return;
        };
        match spawn(program.to_string(), args, 100, 30).await {
            Ok(backend) => {
                let session = moonkale_terminal::Session {
                    id: moonkale_terminal::SessionId::fresh(),
                    title: title.to_string(),
                    cwd: None,
                    backend,
                };
                self.adopt_terminal(session);
            }
            Err(e) => self.set_status(t!(
                self,
                L,
                "program-failed",
                program = program.to_string(),
                error = e
            )),
        }
    }

    /// Hand a running terminal to the terminal panel (it becomes a tab).
    pub fn adopt_terminal(&mut self, session: moonkale_terminal::Session) {
        self.processes
            .adopt_terminals
            .with_mut(|v| v.push(Rc::new(RefCell::new(Some(session)))));
        self.dispatch(Command::ShowPanel("terminal"));
    }

    /// The platform's language-server spawner, if any.
    pub fn spawn_lsp(&self) -> Option<moonkale_lsp::SpawnLsp> {
        self.config.processes.lsp
    }

    /// The platform's language-model provider factory, if any.
    pub fn llm(&self) -> Option<LlmProvider> {
        self.config.runtimes.llm
    }

    /// The platform's Typst compiler, if any.
    pub fn compile_typst(&self) -> Option<CompileTypst> {
        self.config.runtimes.typst
    }

    /// The platform's terminal spawner, if any.
    pub fn spawn_terminal(&self) -> Option<moonkale_terminal::SpawnTerminal> {
        self.config.processes.terminal
    }

    /// The platform's value of a service type an extension defined
    /// ([`WorkspaceConfig::services`]), if the app provides one.
    pub fn service<T: std::any::Any>(&self) -> Option<&'static T> {
        self.config
            .services
            .iter()
            .find_map(|s| (*s as &dyn std::any::Any).downcast_ref::<T>())
    }
}

/// State shared with the terminal and LSP extensions. (Milestone 18 phase 3c: the workspace's state, grouped by area.)
#[derive(Clone, Copy)]
pub struct ProcessesState {
    /// Language-server status for the status bar ("rust-analyzer: indexing…").
    pub lsp_status: Signal<Option<String>>,
    /// Directory for the next `NewTerminal` (set by "New terminal here").
    pub terminal_cwd: Signal<Option<String>>,
    /// Terminal sessions started elsewhere (the `ssh` of a remote session)
    /// that the terminal panel adopts as tabs (Milestone 11).
    pub adopt_terminals: Signal<Vec<Rc<RefCell<Option<moonkale_terminal::Session>>>>>,
}

impl ProcessesState {
    pub(super) fn new() -> Self {
        Self {
            lsp_status: Signal::new_in_scope(None, ScopeId::ROOT),
            terminal_cwd: Signal::new_in_scope(None, ScopeId::ROOT),
            adopt_terminals: Signal::new_in_scope(Vec::new(), ScopeId::ROOT),
        }
    }
}
