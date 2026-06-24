# 🪶 parable — your local coding helper

parable is a small AI coding assistant that runs **entirely on your Mac**. It reads
your code and helps you understand it, spot likely bugs, find duplicated code, and
answer questions about a project — all **offline**. Nothing ever leaves your
computer. No accounts, no API keys, no cloud.

---

## What you need

- A Mac with Apple Silicon (M1 / M2 / M3 / M4).
- About **8 GB of free disk** (the AI model is ~5.5 GB).
- The two things you were handed:
  1. **This folder** (the parable app).
  2. **`Parable_Q4_K_M.gguf`** — the AI model file (~5.5 GB). Pop it somewhere easy,
     like your **Downloads** folder.

You don't need to install anything else yourself — the setup script does it for you.

---

## Setup — one command

1. Open the **Terminal** app (it's in Applications → Utilities, or hit ⌘-Space and
   type "Terminal").
2. Go into this folder and run the installer, pointing it at the model file:

   ```sh
   cd ~/path/to/this/folder
   ./install.sh ~/Downloads/Parable_Q4_K_M.gguf
   ```

The first run takes a few minutes — it sets everything up and builds the app. ☕
When it finishes, there's a **`LocalAgent.app`** sitting in the folder.

---

## Using it — just double-click

**Double-click `LocalAgent.app`** (drag it to your Applications or Desktop first if
you like). A menu opens:

```
🪶  parable — local coding agent
  1) Summarize a project
  2) Review a project for likely bugs
  3) Find duplicate / repeated code
  4) Explain a specific file
  5) Custom task…
  6) Set up Claude Code integration (MCP)
  7) Quit
```

Pick a number. For options **1–5**, it asks you to **drag a project folder into the
window** (then press Enter) — parable reads the files and answers. It stays open so
you can run several things in a row; pick **7** when you're done.

> It can take a little while to think — it's a real AI model running on your own
> machine. The **"working…"** message means it's busy, not stuck.

---

## Using it with Claude Code (optional)

Use Claude Code? You can let it hand small jobs to parable — and the menu sets it
up for you. **Just pick option `6) Set up Claude Code integration`.**

Then start a **new** Claude Code session and ask it to use the parable tools, e.g.
*"use the localagent `summarize_repo` tool"* or *"have localagent review this folder
for bugs."* parable does the reading and reporting; Claude Code makes any actual
edits (with your approval).

(Prefer to do it by hand? `claude mcp add --scope user localagent -- <path-to>/local-coding-agent --mcp`.)

---

## Good to know

- **Private & offline** — your code never leaves your Mac.
- **Safe** — parable can only *read* your files. It can never change, write, or
  delete anything. It just tells you what it finds.
- **It's a helper, not an oracle** — the bug review flags *likely* issues worth a
  look, not a guaranteed or complete list. Always use your own judgment.

---

## If something goes wrong

| Problem | Fix |
| --- | --- |
| "ollama not found" / "can't connect" | Open the **Ollama** app (it gets installed during setup), then try again. |
| "model 'parable' isn't registered" | Re-run `./install.sh ~/Downloads/Parable_Q4_K_M.gguf`. |
| Anything else weird | Re-run `./install.sh ~/Downloads/Parable_Q4_K_M.gguf` — it's safe to run as many times as you like. |

Enjoy your little local coding companion. 🪶
