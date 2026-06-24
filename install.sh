#!/usr/bin/env bash
#
# install.sh — one-command setup for the local coding agent on macOS.
#
# Usage:
#   ./install.sh [path-to-Parable_Q4_K_M.gguf]
#
# Pass the path to the model GGUF the first time (so it can be registered with
# Ollama). If the model is already registered, you can run it with no argument.
#
# It will: install Rust (if missing) → install/start Ollama (if missing) →
# register the model → build the agent. Safe to re-run; it skips steps already done.

set -euo pipefail

MODEL_NAME="parable"
GGUF="${1:-}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

say() { printf "\033[1;36m==>\033[0m %s\n" "$*"; }
err() { printf "\033[1;31mERROR:\033[0m %s\n" "$*" >&2; }

# Generate LocalAgent.app — a double-clickable launcher that opens the menu in
# Terminal. The binary path is baked into the bundled menu script.
build_app() { # <abs-bin-path>
  local bin="$1"
  local app="$HERE/LocalAgent.app"
  rm -rf "$app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

  cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>parable</string>
  <key>CFBundleDisplayName</key><string>parable</string>
  <key>CFBundleIdentifier</key><string>com.localagent.parable</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleExecutable</key><string>LocalAgent</string>
  <key>CFBundlePackageType</key><string>APPL</string>
</dict>
</plist>
PLIST

  cat > "$app/Contents/MacOS/LocalAgent" <<'LAUNCH'
#!/bin/bash
RES="$(cd "$(dirname "$0")/../Resources" && pwd)"
open -a Terminal "$RES/menu.command"
LAUNCH
  chmod +x "$app/Contents/MacOS/LocalAgent"

  sed "s#__BIN__#${bin}#g" "$HERE/launcher/menu.command.tmpl" > "$app/Contents/Resources/menu.command"
  chmod +x "$app/Contents/Resources/menu.command"
}

# 0. Apple Command Line Tools (the compiler/linker Rust needs) ----------------
if ! xcode-select -p >/dev/null 2>&1; then
  say "Installing Apple Command Line Tools — a dialog will pop up; click 'Install' and accept."
  xcode-select --install >/dev/null 2>&1 || true
  printf "    waiting for Command Line Tools to finish installing"
  while ! xcode-select -p >/dev/null 2>&1; do printf "."; sleep 5; done
  printf " done.\n"
fi

# 1. Rust ---------------------------------------------------------------------
if ! command -v cargo >/dev/null 2>&1; then
  say "Installing Rust (rustup)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env" 2>/dev/null || true
say "Rust: $(cargo --version)"

# 2. Ollama -------------------------------------------------------------------
if ! command -v ollama >/dev/null 2>&1; then
  if command -v brew >/dev/null 2>&1; then
    say "Installing Ollama via Homebrew..."
    brew install ollama
  else
    say "Installing Ollama via the official installer..."
    curl -fsSL https://ollama.com/install.sh | sh || true
  fi
fi
if ! command -v ollama >/dev/null 2>&1; then
  err "Ollama still isn't available. Install the app from https://ollama.com/download,"
  err "open it once, then re-run this script."
  exit 1
fi

# Make sure the server is reachable; start it if not.
if ! curl -s -m 3 http://127.0.0.1:11434/api/version >/dev/null 2>&1; then
  say "Starting Ollama server..."
  ( ollama serve >/tmp/ollama-localagent.log 2>&1 & )
  for _ in $(seq 1 30); do
    curl -s -m 2 http://127.0.0.1:11434/api/version >/dev/null 2>&1 && break
    sleep 1
  done
fi
say "Ollama is up."

# 3. Register the model -------------------------------------------------------
# (here-string, not a pipe: `grep -q` + `set -o pipefail` would treat the SIGPIPE
#  from a matched-and-closed `ollama list` as a failure.)
INSTALLED_MODELS="$(ollama list 2>/dev/null || true)"
if grep -q "^${MODEL_NAME}" <<<"$INSTALLED_MODELS"; then
  say "Model '${MODEL_NAME}' already registered — skipping."
else
  if [ -z "$GGUF" ]; then
    err "Model '${MODEL_NAME}' isn't registered yet and no GGUF was given."
    err "Re-run with the model file, e.g.:  ./install.sh ~/Downloads/Parable_Q4_K_M.gguf"
    exit 1
  fi
  if [ ! -f "$GGUF" ]; then err "GGUF file not found: $GGUF"; exit 1; fi
  say "Registering '${MODEL_NAME}' from: $GGUF"
  GGUF_ABS="$(cd "$(dirname "$GGUF")" && pwd)/$(basename "$GGUF")"
  TMP_MODELFILE="$(mktemp)"
  sed "s#^FROM .*#FROM ${GGUF_ABS}#" modelfiles/parable.Modelfile > "$TMP_MODELFILE"
  ollama create "${MODEL_NAME}" -f "$TMP_MODELFILE"
  rm -f "$TMP_MODELFILE"
fi

# 4. Build the agent ----------------------------------------------------------
say "Building the agent (cargo build --release)... first build takes a few minutes."
cargo build --release

BIN="$HERE/target/release/local-coding-agent"

# 5. Generate the double-clickable launcher app -------------------------------
say "Creating LocalAgent.app launcher..."
build_app "$BIN"

printf "\n\033[1;32m✓ Installed.\033[0m\n\n"
echo "Easiest: double-click  LocalAgent.app  (drag it to Applications or your Desktop first)."
echo
echo "Or from the terminal, run the agent on a project:"
echo "    cd /path/to/some/project && \"$BIN\" ."
echo
echo "Use it inside Claude Code (optional):"
echo "    claude mcp add --scope user localagent -- \"$BIN\" --mcp"
