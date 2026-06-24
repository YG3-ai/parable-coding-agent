//! Task prompts shared by the CLI and the MCP tools, so the wording that makes a
//! small local model actually behave (use tools, stay concrete, then deliver)
//! lives in exactly one place.
//!
//! Each public function returns a fully-scaffolded task string ready to hand to
//! the agent loop.

/// Wrap a task-specific instruction with the common tool-use + delivery
/// scaffolding (weak local models need to be told to emit
/// a tool call as bare JSON, to act rather than narrate, and to deliver without
/// asking follow-up questions).
pub fn wrap_task(specific: &str) -> String {
    format!(
        "{specific}\n\n\
         You have two read-only tools: `list_dir` and `read_file`. When you want to use \
         one, reply with ONLY a JSON object like \
         {{\"name\": \"read_file\", \"arguments\": {{\"path\": \"Cargo.toml\"}}}} and nothing \
         else — no narration. Make one tool call per reply. When finished, reply with your \
         final answer as plain prose and stop. Do NOT ask questions; deliver the answer directly."
    )
}

pub fn summarize_repo() -> String {
    wrap_task(
        "Inspect the project in the working directory and summarise what it does, \
         its tech stack, and how it is structured.",
    )
}

pub fn summarize_file(path: &str) -> String {
    wrap_task(&format!("Read the file `{path}` and summarise what it does."))
}

/// Bug review — deliberately conservative wording to suppress hallucinated issues.
pub fn review_bugs(path: &str) -> String {
    wrap_task(&format!(
        "Review the code under `{path}` for likely bugs and issues. Inspect the files with \
         list_dir and read_file. Then list up to 5 of the MOST LIKELY problems, each on its \
         own line as:  <file>:<line> — <the problem> — <why it matters>.  Be concrete and \
         conservative: only report things you can actually see in the code, and do NOT invent \
         issues. If you find nothing notable, say so plainly."
    ))
}

pub fn find_duplicates(path: &str) -> String {
    wrap_task(&format!(
        "Look for duplicated or near-duplicated code under `{path}`. Inspect the files with \
         list_dir and read_file. List groups of similar functions or blocks, each as:  \
         <fileA>:<line> ~ <fileB>:<line> — <what is duplicated> — <how to consolidate>.  Focus \
         on real duplication (the same logic repeated), not superficial similarity. If you \
         find none, say so."
    ))
}

pub fn search_code(query: &str) -> String {
    wrap_task(&format!(
        "Search the project for: {query}. Use list_dir and read_file to find the relevant \
         files and lines, then explain what you found (with file:line references)."
    ))
}

pub fn explain(path: &str, snippet: &str) -> String {
    if snippet.trim().is_empty() {
        wrap_task(&format!(
            "Read the file `{path}` and explain what its code does, clearly and concisely."
        ))
    } else {
        wrap_task(&format!("Explain what this code does:\n\n{snippet}"))
    }
}

pub fn draft_code(instruction: &str) -> String {
    wrap_task(&format!(
        "Write code for the following request. Output only the code (a short explanatory \
         comment is fine):\n\n{instruction}"
    ))
}

/// A freeform user instruction, with the standard scaffolding applied.
pub fn ask(task: &str) -> String {
    wrap_task(task)
}
