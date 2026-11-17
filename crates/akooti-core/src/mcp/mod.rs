//! MCP access control: scoped tokens, folder allowlists and the audit trail.
//!
//! Everything here is metadata and policy; the secret-bearing helpers
//! ([`crate::Vault::mcp_fill_plan`] and friends) are called by the daemon only,
//! never by the MCP process, so a secret can never reach an agent.

pub(crate) mod audit;
pub(crate) mod store;
pub(crate) mod token;

pub use audit::{AuditStatus, McpAction, McpAuditEntry, NewAuditEntry};
pub use token::{IssuedMcpToken, McpPrincipal, McpScope, McpToken, NewMcpToken, TOKEN_PREFIX};
