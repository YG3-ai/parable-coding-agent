//! Server-agnostic tool-call recovery — the "bypass" for inference servers that
//! don't surface native tool calls.
//!
//! Some local servers (notably Ollama 0.30.x) fail to parse a model's tool call
//! into the structured OpenAI `tool_calls` field: the call is left as raw JSON
//! in the message `content`, so a spec-compliant harness never sees it and the
//! agent answers in a single turn without acting. This was confirmed by sending
//! a `tools` request directly to both Ollama endpoints — `tool_calls` came back
//! `null` with the call sitting in `content`.
//!
//! [`ToolCallRecovery`] wraps any [`Model`]. When the underlying model returns
//! no structured tool calls but its text content *looks* like a tool call, it
//! recovers the call(s) from that text. This makes tool use work with any SLM
//! that emits a recognisable tool-call JSON, regardless of the server's own
//! parsing — no per-model Modelfile needed.

use async_trait::async_trait;
use futures::stream::BoxStream;
use harness_core::{Context, Model, ModelDelta, ModelError, ModelInfo, ModelOutput, ToolCall};

/// Wraps a model and recovers tool calls the server left behind in `content`.
pub struct ToolCallRecovery<M: Model> {
    inner: M,
}

impl<M: Model> ToolCallRecovery<M> {
    pub fn new(inner: M) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<M: Model> Model for ToolCallRecovery<M> {
    async fn complete(&self, ctx: &Context) -> Result<ModelOutput, ModelError> {
        let mut out = self.inner.complete(ctx).await?;

        // 1. Strip `<think>…</think>` reasoning blocks that some models (Qwen3,
        //    QwQ, and other reasoning SLMs) emit inline in `content`. We keep
        //    only the post-reasoning answer; an unclosed/empty block yields no
        //    text. This runs first so tool-call recovery sees the real answer.
        if let Some(t) = out.text.take() {
            let cleaned = strip_think(&t);
            out.text = (!cleaned.is_empty()).then_some(cleaned);
        }

        // 2. Only intervene when the server gave us no structured tool calls but
        //    the text content parses as one — i.e. exactly the Ollama failure mode.
        if out.tool_calls.is_empty() {
            if let Some(text) = out.text.as_deref() {
                let recovered = recover_tool_calls(text);
                if !recovered.is_empty() {
                    out.tool_calls = recovered;
                    // The "text" was the tool call itself, not a user-facing
                    // message — drop it so it doesn't leak into the transcript.
                    out.text = None;
                }
            }
        }

        Ok(out)
    }

    async fn stream(
        &self,
        ctx: &Context,
    ) -> Result<BoxStream<'static, Result<ModelDelta, ModelError>>, ModelError> {
        // Recovery applies to the non-streaming path only (the default agent
        // loop uses `complete`). Delegate streaming unchanged.
        self.inner.stream(ctx).await
    }

    fn info(&self) -> ModelInfo {
        self.inner.info()
    }
}

/// Remove `<think>…</think>` reasoning blocks from model text, returning the
/// trimmed remainder. Closed blocks are deleted; a dangling unclosed `<think>`
/// (the model never finished reasoning) drops everything from that point on.
pub fn strip_think(text: &str) -> String {
    let mut s = text.to_string();
    loop {
        let Some(start) = s.find("<think>") else { break };
        match s[start..].find("</think>") {
            Some(rel) => {
                let end = start + rel + "</think>".len();
                s.replace_range(start..end, "");
            }
            None => {
                s.truncate(start);
                break;
            }
        }
    }
    s.trim().to_string()
}

/// Parse zero or more tool calls out of a model's raw text content.
pub fn recover_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    for candidate in extract_json_candidates(text) {
        match serde_json::from_str::<serde_json::Value>(&candidate) {
            // A list of tool calls.
            Ok(serde_json::Value::Array(items)) => {
                for item in &items {
                    if let Some(tc) = value_to_tool_call(item, calls.len()) {
                        calls.push(tc);
                    }
                }
            }
            // A single tool call object.
            Ok(obj @ serde_json::Value::Object(_)) => {
                if let Some(tc) = value_to_tool_call(&obj, calls.len()) {
                    calls.push(tc);
                }
            }
            _ => {}
        }
    }
    calls
}

/// Convert one JSON value into a [`ToolCall`], if it has the expected shape
/// (`{"name": "...", "arguments"|"parameters"|"args": {...}}`).
fn value_to_tool_call(v: &serde_json::Value, idx: usize) -> Option<ToolCall> {
    let obj = v.as_object()?;
    let name = obj.get("name")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    let raw_args = obj
        .get("arguments")
        .or_else(|| obj.get("parameters"))
        .or_else(|| obj.get("args"))
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));

    // Some models nest the arguments as a JSON-encoded string; unwrap if so.
    let args = match raw_args {
        serde_json::Value::String(s) => {
            serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))
        }
        other => other,
    };

    Some(ToolCall {
        id: format!("call_{idx}"),
        name,
        args,
    })
}

/// Pull candidate JSON snippets out of free-form model text. Handles three
/// shapes, in order: `<tool_call>…</tool_call>` blocks (Qwen-style), a fenced
/// ```json code block, and a bare brace/bracket-balanced JSON value.
fn extract_json_candidates(text: &str) -> Vec<String> {
    let tagged = split_tagged(text, "<tool_call>", "</tool_call>");
    if !tagged.is_empty() {
        return tagged;
    }
    // Return EVERY top-level JSON value, not just the first: weak models often
    // echo a `<tool_response>{…}</tool_response>` blob (with no "name") ahead of
    // the real call, so we must keep scanning past it. `value_to_tool_call`
    // filters out anything that isn't actually a tool call.
    all_balanced_json(&strip_code_fence(text))
}

/// Extract the inner text of every `open … close` block.
fn split_tagged(text: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        match after.find(close) {
            Some(end) => {
                out.push(after[..end].trim().to_string());
                rest = &after[end + close.len()..];
            }
            None => {
                out.push(after.trim().to_string());
                break;
            }
        }
    }
    out
}

/// Strip a single leading ```/```json fence and trailing ``` if present.
fn strip_code_fence(text: &str) -> String {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // Drop the optional language tag on the first line.
        let body = rest.splitn(2, '\n').nth(1).unwrap_or("");
        return body.trim_end().strip_suffix("```").unwrap_or(body).to_string();
    }
    t.to_string()
}

/// Return every top-level balanced `{…}` / `[…]` JSON value in `text`,
/// left to right, respecting string literals and escapes.
fn all_balanced_json(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' || bytes[i] == b'[' {
            if let Some(end) = balanced_end(bytes, i) {
                // `{`/`[` and `}`/`]` are ASCII, so these are valid boundaries.
                out.push(text[i..=end].to_string());
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Index of the `}`/`]` that closes the bracket opened at `start`, or `None` if
/// it is never balanced.
fn balanced_end(bytes: &[u8], start: usize) -> Option<usize> {
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };

    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;

    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        if b == b'"' {
            in_str = true;
        } else if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::recover_tool_calls;

    #[test]
    fn bare_json_object() {
        // Exactly what Ollama left in `content` for qwen2.5-coder.
        let calls = recover_tool_calls("{\n  \"name\": \"list_dir\",\n  \"arguments\": {\n    \"path\": \".\"\n  }\n}");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_dir");
        assert_eq!(calls[0].args["path"], ".");
    }

    #[test]
    fn qwen_tool_call_tags() {
        let calls = recover_tool_calls("<tool_call>{\"name\": \"read_file\", \"arguments\": {\"path\": \"Cargo.toml\"}}</tool_call>");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args["path"], "Cargo.toml");
    }

    #[test]
    fn fenced_json_block() {
        let calls = recover_tool_calls("```json\n{\"name\": \"list_dir\", \"arguments\": {\"path\": \".\"}}\n```");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_dir");
    }

    #[test]
    fn skips_echoed_tool_response_and_finds_real_call() {
        // Exactly the messy turn we observed: an echoed <tool_response> blob
        // (no "name") immediately followed by the real tool call.
        let text = "<tool_response>\n{\"content\":\"README.md\",\"limit\":2000,\"path\":\"README.md\"}\n</tool_response>\n{\"name\": \"read_file\", \"arguments\": {\"path\":\"src/main.rs\"}}";
        let calls = recover_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args["path"], "src/main.rs");
    }

    #[test]
    fn strips_think_block_keeps_answer() {
        use super::strip_think;
        assert_eq!(
            strip_think("<think>\nlet me reason\n</think>\nThe answer is 42."),
            "The answer is 42."
        );
        // A tool call after a reasoning block is still recoverable.
        let calls = recover_tool_calls(
            "<think>I should look around</think>\n{\"name\":\"list_dir\",\"arguments\":{\"path\":\".\"}}",
        );
        // recover_tool_calls itself doesn't strip think, but the wrapper does;
        // here we confirm the JSON after the block parses once think is gone.
        assert_eq!(strip_think("<think>x</think>{\"name\":\"list_dir\",\"arguments\":{\"path\":\".\"}}"),
                   "{\"name\":\"list_dir\",\"arguments\":{\"path\":\".\"}}");
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn plain_prose_is_not_a_tool_call() {
        let calls = recover_tool_calls("This project is a local coding agent built with harness-rs.");
        assert!(calls.is_empty());
    }
}
