//! The gate.
//!
//! Classifies each tool call (`ReadOnly | Mutating | Destructive`, using
//! statement classification from the sources for raw queries) and decides:
//! allow, ask the user, or deny — per source, per agent, per workspace policy.
//! Defaults: reads allowed, writes ask, destructive (DROP/DELETE without
//! WHERE, `rm -rf`, schema changes) always ask. Rate/row limits guard against
//! an agent pulling a billion rows into its context.
