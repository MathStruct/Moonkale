//! `trait TerminalBackend { mount, write_output(bytes), resize, on_input }`.

pub mod native;
pub mod xterm;
