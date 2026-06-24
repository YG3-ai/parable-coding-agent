# Local Coding Agent (harness-rs + Ollama)

A minimal coding agent built with the [`harness-rs`](https://github.com/liliang-cn/harness-rs)
agent framework that runs **entirely locally** against [Ollama](https://ollama.com).

It connects to your local Ollama server, gives the model two read-only
filesystem tools (`list_dir` and `read_file`), then asks it to inspect the
current directory and summarise what the project does. The summary is printed to
your terminal.

No API keys, no cloud — everything runs on your machine.

> **Just want to install and use it?** See **[INSTALL.md](INSTALL.md)** for the
> one-command macOS setup (`./install.sh path/to/model.gguf`).

---

## How it works

```
your folder ──► AgentLoop (harness-rs) ──► Ollama (local SLM, default: parable)
                     │  tools: list_dir, read_file
                     └─► reads files, then writes a summary to stdout
```

- **Model adapter:** `harness_models::OpenAiCompat` pointed at
  `providers::OLLAMA` (`http://127.0.0.1:11434/v1`).
- **Loop:** `harness_loop::AgentLoop` runs a ReAct loop, capped at 16 iterations.
- **Tools:** `harness_tools_fs::{ListDir, ReadFile}` — jail-safe, read-only.
- **Tool-call recovery:** [`src/tool_recovery.rs`](src/tool_recovery.rs) wraps the
  model so tool calls work even when the server doesn't surface them natively, and
  strips `<think>` reasoning from reasoning models (see
  [Tool-call recovery](#tool-call-recovery-the-ollama-bypass) below).

> **Tested with several local SLMs.** The default is **`parable`** (a Qwen-based 9B,
> best summary quality in our testing); see the [Model notes](#model-notes) below.

> Crate names: on crates.io these are published as `harness-rs-*`
> (e.g. `harness-rs-core`). `Cargo.toml` uses Cargo's `package = ` rename so the
> code can `use harness_core`, `use harness_loop`, etc. The bare `harness-core`
> name on crates.io is an **unrelated** project — don't depend on it.

### Configuration (env vars)

Both the model and the endpoint are overridable at runtime — no recompile needed,
so you can swap in other local SLMs freely:

| Variable | Default | Purpose |
| --- | --- | --- |
| `OLLAMA_MODEL` | `parable` | Model tag (must match `ollama list`). |
| `OLLAMA_URL` | `http://127.0.0.1:11434/v1` | Any OpenAI-compatible endpoint. |
| `HARNESS_LOG` | `warn` | Log level (`info`/`debug`) for harness internals. |

```sh
OLLAMA_MODEL=gemma4-coding cargo run   # the faster alternative
```

### Tool-call recovery (the Ollama bypass)

For an agent to *act*, the model's tool call must come back in the OpenAI
`tool_calls` field. Some local servers — **notably Ollama 0.30.x** — fail to parse
certain models' calls into that field and instead leave the call as raw JSON in
the message `content`. (Verified by hitting both Ollama's `/v1/chat/completions`
and native `/api/chat` directly: `tool_calls` came back `null` with the call
sitting in `content`.) A spec-compliant harness never sees it, so the agent
answers in one turn without doing anything.

[`src/tool_recovery.rs`](src/tool_recovery.rs) is a `Model` wrapper that fixes
this **server-side-agnostically**:

- **Tool-call recovery** — whenever the backend returns no structured tool calls
  but the text content *looks* like one, it recovers the call(s) from the content.
  It handles bare JSON, `<tool_call>…</tool_call>` (Qwen-style) and fenced
  ```` ```json ```` blocks, and skips echoed `<tool_response>` blobs.
- **`<think>` stripping** — reasoning models (Qwen3 / QwQ style, including
  Qwythos) wrap their output in `<think>…</think>`. The wrapper strips those
  blocks so the real answer (and any tool call after the reasoning) is what the
  loop sees.

This makes tool use work with **any SLM that emits a recognisable tool-call JSON**,
reasoning model or not. Unit tests cover the parsing (`cargo test`).

---

## Prerequisites

### 1. Install Rust

If you don't already have Rust:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then restart your terminal (or `source "$HOME/.cargo/env"`) and verify:

```sh
cargo --version
```

If you already have Rust, make sure it's current — `harness-rs` requires a
recent toolchain (Rust 1.92+):

```sh
rustup update
```

### 2. Install Ollama and set up a model

Install Ollama from <https://ollama.com/download> (or, on macOS, `brew install ollama`),
then start the server (if it isn't already running as a background service):

```sh
ollama serve
```

You have two ways to give the agent a model. **Either is fine** — just point
`OLLAMA_MODEL` at whatever tag you end up with (`ollama list` shows them).

**Option A — pull a ready-made model (easiest).** Any tool-capable instruct model
works; function-calling models chain most reliably:

```sh
ollama pull llama3.1:8b
ollama list
OLLAMA_MODEL=llama3.1:8b cargo run
```

**Option B — register a local GGUF via a Modelfile.** This is how the default
`parable` (and `gemma4-coding`) are set up. Ready-made recipes live in
[`modelfiles/`](modelfiles/) — **edit the `FROM` line to point at your `.gguf`**,
then register:

```sh
ollama create parable -f modelfiles/parable.Modelfile
ollama list
cargo run            # uses parable by default
```

> The Modelfiles encode hard-won lessons: reasoning models (the Qwen-based weights
> behind `parable`) need a tool-compatible chat template and a **non-thinking
> prefill**, or they crash Ollama's tool-parser / return empty answers.
> `parable.Modelfile` has both.

> **Tip on model behaviour:** small models (≤7B) are inconsistent at tool
> calling — they sometimes narrate ("I'll now read X") instead of emitting a
> call. The [recovery shim](#tool-call-recovery-the-ollama-bypass) fixes the
> *format* problem; model *discipline* (actually emitting a call, then
> delivering an answer) still depends on the model and the prompt.

---

## Run it

From this project folder, with `ollama serve` running:

```sh
cargo run
```

The first build downloads and compiles the dependencies, so it takes a minute or
two. Subsequent runs are fast.

You'll see the agent connect, read the files in this directory, and print a
summary of the project.

### Give it your own task

Pass a freeform instruction, or use a **named task**:

```sh
cargo run -- "Read Cargo.toml and list every dependency"   # freeform
cargo run -- --task summary                                # summarise the project
cargo run -- --task bugs                                   # flag likely bugs
cargo run -- --task duplicates                             # find repeated code
cargo run -- --task explain --path src/main.rs             # explain a file
cargo run -- --task search --query "tool recovery"         # find + explain matches
```

### The double-click launcher

`install.sh` also generates **`LocalAgent.app`** — double-click it (drag a project
folder into the window when prompted) for a friendly menu: summarise, review for
bugs, find duplicates, explain a file, or a custom task. No terminal commands to
remember. The menu maps directly onto the `--task` flags above.

---

## Use with Claude Code (MCP)

The agent can also run as an **MCP server**, exposing a few **read-only** tools that
Claude Code (or Cursor / Codex) can delegate to — so the big cloud model offloads
cheap, private, offline jobs to your local SLM, and applies changes itself.

Register it with Claude Code:

```sh
cargo build --release
claude mcp add --scope user localagent -- "$(pwd)/target/release/local-coding-agent" --mcp
```

The tools it exposes (all read-only — they return text, never write):

| Tool | Args | What it does |
| --- | --- | --- |
| `summarize_repo` | – | Summarise the project in the working directory |
| `summarize_file` | `path` | Summarise one file |
| `review_bugs` | `path` | Flag likely bugs/issues (conservative) |
| `find_duplicates` | `path` | Find duplicated / repeated code |
| `search_code` | `query` | Find and explain matches |
| `explain_code` | `path` or `snippet` | Explain what code does |
| `draft_code` | `instruction` | Draft code (returned as **text**; not written) |
| `ask_local` | `task` | General fallback |

**Safety:** the local agent only ever reads and returns text — it has no
write/edit/shell tools, so it *cannot* modify your files. Claude Code applies any
changes itself, with your approval. Each tool is declared `ToolRisk::ReadOnly`.

---

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `agent loop failed …` / connection refused | Start the server: `ollama serve`. Check it's reachable at `http://127.0.0.1:11434`. |
| `model '…' not found` | Pull it (`ollama pull <tag>`), or point `OLLAMA_MODEL` at a tag from `ollama list`. |
| cargo complains about the Rust version / edition 2024 | `rustup update` to get a current stable toolchain. |
| Agent answers in **1 iteration without reading files** | The model didn't emit a tool call at all (it narrated instead). Try a function-calling model via `OLLAMA_MODEL=llama3.1:8b`. The recovery shim only helps when the model *does* emit call JSON. |
| The summary is thin or the agent stops early | Small models can be terse. Try a larger model (`OLLAMA_MODEL=…`) or raise `MAX_ITERS` in [`src/main.rs`](src/main.rs). |
| Want to see what the agent is doing | Run with `HARNESS_LOG=info cargo run`. |

---

## Model notes

Several local SLMs were benchmarked as drivers. Short version:

| Model | Output quality | Setup | Speed | Pick it for |
| --- | --- | --- | --- | --- |
| `parable` (reasoning, **default**) | best — most detailed | most | slowest | richest analysis |
| `gemma4-coding` (9B) | good, reliable | low | fast | low-friction default |
| `qwen2.5-coder:7b` | decent | low | fast | lightweight |

---

## Project layout

```
.
├── Cargo.toml                  # dependencies + crate metadata
├── src/
│   ├── main.rs                 # the agent: connect to Ollama, register tools, run, print
│   ├── mcp.rs                  # MCP server mode (8 read-only tools for Claude Code)
│   ├── prompts.rs              # shared task prompts (CLI + MCP)
│   └── tool_recovery.rs        # Model wrapper: recover tool calls + strip <think> (+ tests)
├── modelfiles/
│   ├── parable.Modelfile       # default model recipe (tool template + no-think prefill)
│   └── gemma4-coding.Modelfile # a faster alternative
├── launcher/menu.command.tmpl  # template for the double-click LocalAgent.app menu
├── install.sh                  # one-command macOS setup
├── GUIDE.md                    # friendly install + usage guide
├── README.md
└── LICENSE
```
