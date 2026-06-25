//! A tiny local coding agent built on the `harness-rs` framework.
//!
//! It connects to a **local Ollama** server (which exposes an OpenAI-compatible
//! API), hands the model two read-only filesystem tools (`list_dir` and
//! `read_file`), and asks it to inspect the current directory and summarise what
//! the project does. The agent's answer is printed to the console.
//!
//! Usage:
//! ```sh
//! localagent                      # summarise the current directory
//! localagent "your own task"      # a custom freeform instruction
//! localagent --task bugs          # named task: summary | bugs | duplicates | explain | search
//! localagent --mcp                # run as an MCP server (for Claude Code etc.)
//! ```

mod mcp;
mod prompts;
mod tool_recovery;

use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use harness_context::default_world;
use harness_core::{Model, Task};
use harness_loop::{AgentLoop, Outcome};
use harness_models::{providers, OpenAiCompat};
use harness_tools_fs::{ListDir, ReadFile};

use tool_recovery::ToolCallRecovery;

/// Default Ollama model tag, used when `OLLAMA_MODEL` is not set. Must match a
/// tag from `ollama list`. Override per-run without editing code, e.g.:
///   OLLAMA_MODEL=gemma4-coding cargo run
///
/// `parable` (a small fable 🪶) is the default — a Qwen-based 9B reasoning model
/// that produced the best, most detailed summary in testing. It's set up via a
/// custom Modelfile (modelfiles/parable.Modelfile): a tool-compatible template
/// plus a non-thinking prefill. `gemma4-coding` is the faster alternative.
///
/// Tool use does NOT depend on the server's native `tool_calls` support: the
/// ToolCallRecovery wrapper (src/tool_recovery.rs) recovers calls from message
/// content and strips `<think>` reasoning, so any SLM that emits call JSON works.
const DEFAULT_MODEL: &str = "parable";

/// Upper bound on agent loop iterations (each iteration = one model turn, which
/// may include tool calls). Keeps a small local model from looping forever.
const MAX_ITERS: u32 = 16;

/// 🪶 parable — a small, local, offline coding agent.
#[derive(Parser, Debug)]
#[command(name = "localagent", about = "parable — a local, offline coding agent")]
struct Cli {
    /// A freeform instruction (e.g. "explain main.rs"). Ignored if --task is set.
    instruction: Option<String>,

    /// Run as an MCP server (stdio) for Claude Code / Cursor / Codex.
    #[arg(long)]
    mcp: bool,

    /// Run a named task: summary | bugs | duplicates | explain | search.
    #[arg(long, value_name = "KIND")]
    task: Option<String>,

    /// Path/folder a named task is scoped to (default: the whole project).
    #[arg(long, default_value = ".")]
    path: String,

    /// Query text for `--task search`.
    #[arg(long)]
    query: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Resolve the model tag and endpoint (env-overridable so you can swap in
    // other local SLMs without recompiling). providers::OLLAMA is
    // "http://127.0.0.1:11434/v1".
    let model_tag = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let endpoint = std::env::var("OLLAMA_URL").unwrap_or_else(|_| providers::OLLAMA.to_string());

    // MCP server mode: stdout is the protocol channel — print nothing else to it.
    if cli.mcp {
        return mcp::serve_mcp(model_tag, endpoint, MAX_ITERS).await;
    }

    // CLI mode: build the task prompt (named --task, freeform instruction, or
    // the default "summarise this project"). All prompts live in src/prompts.rs.
    let description = match cli.task.as_deref() {
        None => match cli.instruction.as_deref() {
            Some(instr) => prompts::ask(instr),
            None => prompts::summarize_repo(),
        },
        Some("summary") | Some("summarize") => prompts::summarize_repo(),
        Some("bugs") => prompts::review_bugs(&cli.path),
        Some("duplicates") | Some("dupes") => prompts::find_duplicates(&cli.path),
        Some("explain") => prompts::explain(&cli.path, ""),
        Some("search") => prompts::search_code(cli.query.as_deref().unwrap_or("")),
        Some(other) => {
            eprintln!(
                "unknown --task '{other}'. Use one of: summary | bugs | duplicates | explain | search."
            );
            std::process::exit(2);
        }
    };

    let workspace =
        std::env::current_dir().context("could not determine the current working directory")?;

    // Point the OpenAI-compatible adapter at the local Ollama server, then wrap
    // it so we recover tool calls the server left in `content` (src/tool_recovery.rs).
    let backend = OpenAiCompat::with_key(endpoint.clone(), model_tag, "ollama");
    let model = ToolCallRecovery::new(backend);

    let info = model.info();
    println!("🪶 parable — local coding agent");
    println!("  model:     {} (via {})", info.model, info.provider);
    println!("  endpoint:  {endpoint}");
    println!("  workspace: {}\n", workspace.display());

    let agent = AgentLoop::new(model)
        .with_tool(Arc::new(ListDir))
        .with_tool(Arc::new(ReadFile));

    let mut world = default_world(&workspace);
    let task = Task {
        description,
        source: None,
        deadline: None,
    };

    let outcome = agent
        .run_with_max_iters(task, &mut world, MAX_ITERS)
        .await
        .context(
            "agent loop failed — make sure Ollama is running (`ollama serve`) and the \
             model is registered (check `ollama list`).",
        )?;

    // 6. Print whatever the agent produced.
    println!("\n──────── result ────────");
    match outcome {
        Outcome::Done { text, iters, .. } => {
            println!("(completed in {iters} iteration(s))\n");
            println!(
                "{}",
                text.unwrap_or_else(|| "<the agent finished without returning any text>".into())
            );
        }
        Outcome::BudgetExhausted {
            last_text, iters, ..
        } => {
            eprintln!("(stopped after reaching the {iters}-iteration limit)\n");
            println!(
                "{}",
                last_text.unwrap_or_else(|| "<no partial answer was produced>".into())
            );
        }
    }

    Ok(())
}
