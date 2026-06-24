//! MCP server mode — exposes the local agent as a small set of **read-only**
//! tools that Claude Code (or Cursor / Codex) can delegate to.
//!
//! Design: the local SLM does the cheap/offline work —
//! reading, searching, summarising, drafting — and *returns text*. It never
//! writes; the orchestrator (Claude Code, with its own user-approval) applies
//! any changes. Every tool here is [`ToolRisk::ReadOnly`].
//!
//! Each tool is a harness `Tool` whose `invoke` runs a local `AgentLoop` (the
//! same one the CLI uses: the local `parable` model via Ollama + the two filesystem tools + the
//! tool-call/`<think>` recovery shim) and returns the final answer.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use harness_core::{Task, Tool, ToolError, ToolResult, ToolRisk, ToolSchema, World};
use harness_context::default_world;
use harness_loop::{AgentLoop, Outcome};
use harness_models::OpenAiCompat;
use harness_tools_fs::{ListDir, ReadFile};
use harness_mcp::McpServer;

use crate::prompts;
use crate::tool_recovery::ToolCallRecovery;

/// Which granular capability a [`LocalAgentTool`] exposes.
#[derive(Clone, Copy)]
enum Kind {
    SummarizeRepo,
    SummarizeFile,
    SearchCode,
    ExplainCode,
    DraftCode,
    AskLocal,
    ReviewBugs,
    FindDuplicates,
}

const KINDS: [Kind; 8] = [
    Kind::SummarizeRepo,
    Kind::SummarizeFile,
    Kind::SearchCode,
    Kind::ExplainCode,
    Kind::DraftCode,
    Kind::AskLocal,
    Kind::ReviewBugs,
    Kind::FindDuplicates,
];

/// A read-only MCP tool that delegates to a local agent run.
struct LocalAgentTool {
    kind: Kind,
    schema: ToolSchema,
    model_tag: String,
    endpoint: String,
    max_iters: u32,
}

impl LocalAgentTool {
    fn new(kind: Kind, model_tag: &str, endpoint: &str, max_iters: u32) -> Self {
        let (name, description, input) = match kind {
            Kind::SummarizeRepo => (
                "summarize_repo",
                "Summarise the project in the working directory using the local model: \
                 what it does, its tech stack, and how it is structured.",
                json!({ "type": "object", "properties": {} }),
            ),
            Kind::SummarizeFile => (
                "summarize_file",
                "Read a single file and summarise what it does, using the local model.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path, relative to the working directory." }
                    },
                    "required": ["path"]
                }),
            ),
            Kind::SearchCode => (
                "search_code",
                "Search the project for a symbol, string, or concept and explain the \
                 matches, using the local model.",
                json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to search for." }
                    },
                    "required": ["query"]
                }),
            ),
            Kind::ExplainCode => (
                "explain_code",
                "Explain what some code does, using the local model. Provide a file \
                 `path` to explain, or a `snippet` directly.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to a file to explain (optional if 'snippet' given)." },
                        "snippet": { "type": "string", "description": "Code to explain (optional if 'path' given)." }
                    }
                }),
            ),
            Kind::DraftCode => (
                "draft_code",
                "Draft code for an instruction using the local model. Returns code as \
                 TEXT only — it does NOT write any files.",
                json!({
                    "type": "object",
                    "properties": {
                        "instruction": { "type": "string", "description": "What to write." }
                    },
                    "required": ["instruction"]
                }),
            ),
            Kind::AskLocal => (
                "ask_local",
                "Ask the local model to do a coding task in the working directory \
                 (general fallback). Read-only: it can read files but never writes.",
                json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string", "description": "The task or question." }
                    },
                    "required": ["task"]
                }),
            ),
            Kind::ReviewBugs => (
                "review_bugs",
                "Review code for likely bugs and issues using the local model. Returns a \
                 short, conservative list (file:line — problem — why). Read-only.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File or folder to review (default: whole project)." }
                    }
                }),
            ),
            Kind::FindDuplicates => (
                "find_duplicates",
                "Find duplicated or near-duplicated code using the local model. Returns \
                 groups of similar blocks with locations and consolidation hints. Read-only.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File or folder to scan (default: whole project)." }
                    }
                }),
            ),
        };

        Self {
            kind,
            schema: ToolSchema {
                name: name.to_string(),
                description: description.to_string(),
                input,
            },
            model_tag: model_tag.to_string(),
            endpoint: endpoint.to_string(),
            max_iters,
        }
    }

    /// Build the agent task prompt from the call arguments. All wording lives in
    /// src/prompts.rs so the CLI and the MCP tools behave identically.
    fn build_task(&self, args: &Value) -> String {
        let arg = |k: &str| {
            args.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string()
        };
        // For folder-scoped tasks, an empty path means "the whole project".
        let path_or_root = |k: &str| {
            let p = arg(k);
            if p.is_empty() { ".".to_string() } else { p }
        };

        match self.kind {
            Kind::SummarizeRepo => prompts::summarize_repo(),
            Kind::SummarizeFile => prompts::summarize_file(&arg("path")),
            Kind::SearchCode => prompts::search_code(&arg("query")),
            Kind::ExplainCode => prompts::explain(&arg("path"), &arg("snippet")),
            Kind::DraftCode => prompts::draft_code(&arg("instruction")),
            Kind::AskLocal => prompts::ask(&arg("task")),
            Kind::ReviewBugs => prompts::review_bugs(&path_or_root("path")),
            Kind::FindDuplicates => prompts::find_duplicates(&path_or_root("path")),
        }
    }
}

#[async_trait]
impl Tool for LocalAgentTool {
    fn name(&self) -> &str {
        &self.schema.name
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn risk(&self) -> ToolRisk {
        // Every tool only reads + returns text. Never writes. This is the
        // safety guarantee (local agent only ever reads), expressed to MCP clients.
        ToolRisk::ReadOnly
    }

    async fn invoke(&self, args: Value, world: &mut World) -> Result<ToolResult, ToolError> {
        let backend = OpenAiCompat::with_key(self.endpoint.clone(), self.model_tag.clone(), "ollama");
        let model = ToolCallRecovery::new(backend);
        let agent = AgentLoop::new(model)
            .with_tool(Arc::new(ListDir))
            .with_tool(Arc::new(ReadFile));

        let task = Task {
            description: self.build_task(&args),
            source: None,
            deadline: None,
        };

        // Errors are returned as an unsuccessful ToolResult (so the MCP client
        // sees the message) rather than failing the whole tool call.
        match agent.run_with_max_iters(task, world, self.max_iters).await {
            Ok(Outcome::Done { text, .. }) => Ok(ok_text(text.unwrap_or_default())),
            Ok(Outcome::BudgetExhausted { last_text, .. }) => Ok(ok_text(
                last_text.unwrap_or_else(|| "(no answer; iteration budget exhausted)".into()),
            )),
            Err(e) => Ok(ToolResult {
                ok: false,
                content: json!(format!("local agent error: {e}")),
                trace: None,
            }),
        }
    }
}

fn ok_text(text: String) -> ToolResult {
    ToolResult {
        ok: true,
        content: json!(text),
        trace: None,
    }
}

/// Run the MCP stdio server, exposing the granular read-only agent tools.
pub async fn serve_mcp(model_tag: String, endpoint: String, max_iters: u32) -> anyhow::Result<()> {
    let tools: Vec<Arc<dyn Tool>> = KINDS
        .iter()
        .map(|&kind| {
            Arc::new(LocalAgentTool::new(kind, &model_tag, &endpoint, max_iters)) as Arc<dyn Tool>
        })
        .collect();

    // Root the agent's filesystem tools at the directory the MCP server was
    // launched in (Claude Code launches it inside the project).
    let mut world = default_world(std::env::current_dir()?);

    McpServer::new("localagent", env!("CARGO_PKG_VERSION"))
        .with_tools(tools)
        .serve_stdio(&mut world)
        .await
}
