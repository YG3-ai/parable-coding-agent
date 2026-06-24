# Installing the Local Coding Agent (macOS)

A friendly, offline coding assistant that runs entirely on your Mac. This guide
gets you from zero to running in about one command. No accounts, no API keys —
nothing leaves your machine.

## What you need

- An **Apple Silicon Mac** (M1–M4). It'll be nice and fast. ⚡
- About **8 GB of free disk** (the AI model itself is ~5.5 GB).
- Two things, handed to you by whoever shared this:
  1. **This project folder** (the code).
  2. The **model file**: `Parable_Q4_K_M.gguf` (~5.5 GB) — easiest to receive via **AirDrop**.

You do **not** need to install Rust or Ollama yourself — the installer handles them.

## Install — one command

1. Put the project folder somewhere handy (e.g. `~/localagent`) and the `.gguf`
   somewhere you'll remember (e.g. `~/Downloads`).
2. Open **Terminal**, go into the project folder, and run the installer with the
   path to your model file:

   ```sh
   cd ~/localagent
   ./install.sh ~/Downloads/Parable_Q4_K_M.gguf
   ```

That's it. The script will install Rust (if needed), install & start Ollama (if
needed), register the AI model, and build the agent. **The first build takes a few
minutes** — grab a coffee. ☕ Re-running later is instant, and you can drop the
`.gguf` argument once the model is registered.

> No scary "unidentified developer" pop-ups: because the app is **built on your own
> Mac**, macOS trusts it — there's nothing to un-quarantine.

## Use it

**Easiest — the launcher app.** The installer creates **`LocalAgent.app`** in the
project folder. Double-click it (drag it to your Applications or Desktop first if
you like). A menu appears — **drag a project folder into the window**, then pick:

```
🪶  parable — local coding agent
  1) Summarize the project
  2) Review for likely bugs
  3) Find duplicate / repeated code
  4) Explain a specific file
  5) Custom task…
```

That's the whole experience — no terminal commands to remember.

**Or from the terminal.** The installer prints the exact path at the end. To
summarise any project:

```sh
cd ~/some/code/project
~/localagent/target/release/local-coding-agent .
```

Or just ask it something:

```sh
~/localagent/target/release/local-coding-agent "explain what main.rs does"
```

**Handy:** make a short alias so you can just type `localagent`:

```sh
echo 'alias localagent="$HOME/localagent/target/release/local-coding-agent"' >> ~/.zshrc
source ~/.zshrc
# now:  localagent .
```

## Use it inside Claude Code (optional)

Let Claude Code hand cheap/offline jobs to your local model:

```sh
claude mcp add localagent -- "$HOME/localagent/target/release/local-coding-agent" --mcp
```

Then start a Claude Code session in any project and ask it to use the `summarize_repo`,
`search_code`, or `ask_local` tools. Your local model reads and drafts; Claude Code
makes the actual edits (with your approval). The local tools are **read-only — they
can never change your files**.

## Good to know

- **Fully offline & private** after install — your code never leaves your Mac.
- On an M-series Mac the model is quick. (It's only slow on old Intel Macs.)
- Want it snappier / lighter? Register a smaller model and select it per-run with
  `OLLAMA_MODEL=…` (see the project README).

## Troubleshooting

| Problem | Fix |
| --- | --- |
| "ollama not found" or "can't connect" | Open the **Ollama** app (or run `ollama serve`), then re-run `./install.sh`. |
| Cargo / Rust version error | Run `rustup update` and re-run the installer. |
| "model 'parable' isn't registered" | Re-run with the model path: `./install.sh /path/to/Parable_Q4_K_M.gguf`. |
| It's slow | You're probably on an older/Intel Mac. Try a smaller model via `OLLAMA_MODEL=…`. |
