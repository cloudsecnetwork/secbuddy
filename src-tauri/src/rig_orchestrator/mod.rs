//! Rig-based agent orchestration. This is the only orchestration backend;
//! `send_message` in `lib.rs` calls `run_agent_loop` directly.
//!
//! Internally we use Rig's low-level `CompletionModel` trait so we can
//! intercept `AssistantContent::ToolCall` items and route them through the
//! manual approval gate in `crate::lib::record_approval_and_execute`.
//! Local + MCP tool execution lives in `tool_runner` / `mcp_client` so
//! cancellation, PID tracking, and timeouts work without re-implementation.

pub mod approval;
pub mod definitions;
pub mod events;
mod loop_impl;
pub mod provider;
pub mod tool_adapter;

pub use loop_impl::run_agent_loop;
