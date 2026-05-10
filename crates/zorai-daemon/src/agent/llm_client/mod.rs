//! LLM API client with SSE streaming support.
//!
//! Supports two API formats:
//! - OpenAI-compatible (`/chat/completions` with Bearer auth) — covers most providers
//! - Anthropic Messages API (`/v1/messages` with `x-api-key` header)

mod anthropic;
mod anthropic_batches;
mod anthropic_request_fields;
mod anthropic_response_types;
mod anthropic_stream_message_start;
mod anthropic_stream_stop;
mod anthropic_stream_upstream_message;
mod anthropic_stream_usage;
mod api_types;
mod helpers;
mod native_assistant;
mod openai_responses_protocol;
mod openai_runtime;
mod openai_sse;
mod openai_transport;
mod prelude;
mod public_api;

pub use anthropic::*;
pub use anthropic_batches::*;
pub use anthropic_request_fields::*;
pub use anthropic_response_types::*;
pub use anthropic_stream_message_start::*;
pub use anthropic_stream_stop::*;
pub use anthropic_stream_upstream_message::*;
pub use anthropic_stream_usage::*;
pub use api_types::*;
pub use helpers::*;
pub use native_assistant::*;
pub use openai_responses_protocol::*;
pub use openai_runtime::*;
pub use openai_sse::*;
pub use openai_transport::*;
pub use prelude::*;
pub use public_api::*;

#[cfg(test)]
mod tests;
