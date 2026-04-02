//! LLM API client with SSE streaming support.
//!
//! Supports two API formats:
//! - OpenAI-compatible (`/chat/completions` with Bearer auth) — covers most providers
//! - Anthropic Messages API (`/v1/messages` with `x-api-key` header)

include!("prelude.rs");
include!("api_types.rs");
include!("public_api.rs");
include!("openai_transport.rs");
include!("native_assistant.rs");
include!("openai_runtime.rs");
include!("openai_sse.rs");
include!("anthropic.rs");
include!("helpers.rs");

#[cfg(test)]
mod tests;
