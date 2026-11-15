//! Akooti MCP.
//!
//! Model Context Protocol server exposed to coding agents, plus the
//! Zero-Leak injection engines (browser extension bridge and CDP).
//!
//! Tools never return decrypted secrets: they only expose metadata and
//! action results (see `docs/regles.md` §1.1).

#![forbid(unsafe_code)]
