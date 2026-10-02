//! `Workspace`: terminals, programs, language servers, Typst, the LLM provider, git. Split out of `workspace.rs` in Milestone 18 phase 3b
//! (no API change; phase 3c turns the areas into services).

use super::*;

impl Workspace {
    /// Can this platform run a program in a terminal tab?
    pub fn can_run_program(&self) -> bool {
        self.config.processes.program.is_some()
    }

    /// Start `program args…` under a PTY and hand it to the terminal panel
    /// as a tab titled `title` (Milestone 15: `claude auth login`).
    pub async fn run_in_terminal(mut self, title: &str, program: &str, args: Vec<String>) {
        let Some(spawn) = self.config.processes.program else {
            self.set_status("Running a program in a terminal is not available on this platform");
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
            Err(e) => self.set_status(format!("Could not start {program}: {e}")),
        }
    }

    /// Hand a running terminal to the terminal panel (it becomes a tab).
    pub fn adopt_terminal(&mut self, session: moonkale_terminal::Session) {
        self.adopt_terminals
            .with_mut(|v| v.push(Rc::new(RefCell::new(Some(session)))));
        self.dispatch(Command::ShowPanel("terminal"));
    }

    pub fn spawn_lsp(&self) -> Option<moonkale_lsp::SpawnLsp> {
        self.config.processes.lsp
    }

    pub fn llm(&self) -> Option<LlmProvider> {
        self.config.runtimes.llm
    }

    pub fn compile_typst(&self) -> Option<CompileTypst> {
        self.config.runtimes.typst
    }

    /// The platform's terminal spawner, if any.
    pub fn spawn_terminal(&self) -> Option<moonkale_terminal::SpawnTerminal> {
        self.config.processes.terminal
    }

    /// The platform's git runner, if any.
    pub fn git(&self) -> Option<GitRun> {
        self.config.processes.git
    }
}
