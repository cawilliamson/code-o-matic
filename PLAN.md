# PLAN: bringing code-o-matic to parity with pi's basic functionality

## 0. Context and scope

### Goal
Make `code-o-matic` (com) usable as a **drop-in replacement** for the interactive
coding-agent side of pi (`@earendil-works/pi-coding-agent`), with two stated
exceptions:

- **Extensions** — pi's extension SDK is explicitly out of scope. We are the
  extension: native code, not a plugin host.
- **Config paths / env-var names** — com keeps its own naming
  (`COM_*`, `.com/`, `SOUL.md`/`AGENTS.md` at repo root) rather than pi's
  (`PI_*`, `~/.pi`).

Everything else that a user of `pi` interactive mode relies on day-to-day is in
scope: the agent loop, tool set, system prompt behaviour, sessions/durability,
TUI behaviour, model/provider handling, permissions, and protocol modes.

### Current state (verified against this repo, Aug 2025)
| Area | code-o-matic today |
|------|--------------------|
| Agent loop | async loop, sequential tool calls, single `HttpLlmClient` |
| Tools | `read`, `write`, `edit`, `bash` only |
| System prompt | single identity line + AGENTS.md/SOUL.md injection |
| Provider | one hardcoded "twobobs" HTTP gateway (deepseek model) |
| Sessions | `.com/sessions` jsonl repo, branching, compaction, continuity |
| Skills | markdown + frontmatter, triggers, scope detection |
| TUI | `crossterm` terminal UI, context/full views |
| Commands | `/help /clear /new /model /undo /quit /context /full` |
| Permissions | `PermissionClass` enum exists, no runtime enforcement |
| Modes | interactive TUI + single prompt (non-interactive) |
| Streaming | token stream via `complete_stream`, no parallel tools |

### pi baseline (verified against github.com/earendil-works/pi main)
- Agent runtime with **multi-provider** LLM (OpenAI, Anthropic, Google, …).
- Tools: `read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`, `edit-diff`.
- **Tool schemas carry rich usage guidance** ("use offset/limit for large
  files", "continue with offset until complete", uniqueness rules for edits) and
  the system prompt emits **per-tool guidelines** depending on which tools are
  active.
- Sessions: sqlite backends, `fork`, `resume`, `export`, `import`,
  `changelog`, `rename`, session picker.
- Slash commands: `changelog clone compact copy export fork hotkeys import
  login logout model name new quit reload resume scoped-models session settings
  share tree trust`.
- Project trust / permission manager, auth management (`login`/`logout`),
  model registry + `scoped-models`, keybindings, telemetry, RPC mode, print
  mode, automatic compaction.

---

## 1. Priority 1 (do first): stop gratuitous tool use

This is the user's named pain point and it is a **prompt + schema** problem, not
a loop problem. Root causes observed:

- com's system prompt is a single identity line with **zero usage guidelines**.
- com's tool schemas are minimal (`"relative path inside the repository"`) with
  **no instruction on when to call a tool, how to avoid re-reading, or how to
  limit output**. The model therefore calls `bash`/`read` speculatively.
- There is no guidance channel for "answer directly unless you need to
  investigate".

### 1.1 Enrich every tool schema with usage guidance
Port the instructive `description` fields from pi's `src/core/tools/*.ts` into
the native schemas in `crates/code_o_matic/src/builtin/*`:

- `read`: "Read the contents of a file. For text files output is truncated to
  N lines / NKB. Use offset/limit for large files; continue with offset until
  complete." Also note image files are sent as attachments once image support
  lands (see §3).
- `bash`: announce stdout/stderr capture, output truncation to last N
  lines/NKB, optional timeout.
- `write`: overwrite semantics; parent dirs created.
- `edit`: exact-match semantics, first-occurrence-replaced today vs pi's
  unique-match requirement (§2.2 → align with pi's uniqueness/multi-occurrence
  guidance).

### 1.2 Add usage guidelines to the system prompt
Extend `system_prompt::default_system_prompt` to emit *conditional* guidelines
mirroring pi's `buildSystemPrompt` logic:

- `- Be concise in your responses`
- `- Show file paths clearly when working with files`
- If `bash` present but `grep`/`find`/`ls` absent: `- Use bash for file
  operations like ls, rg, find`
- Add an explicit anti-speculation line pi relies on:
  `- Do not call tools unless you need to inspect or change state; answer from
  what you already know first`

### 1.3 Acceptance
With `grep`/`find`/`ls` absent, trivial questions must produce a text-only
assistant turn with **zero** tool calls (regression test asserting no tool calls
on a fact-answer prompt). See §6 for how to make this testable.

---

## 2. Priority 2: tool-set parity

Missing relative to pi: `grep`, `find`, `ls`, `edit-diff`. `read` lacks image
support.

### 2.1 Add `grep`, `find`, `ls`
Native tools under `crates/code_o_matic/src/builtin/`, registered in
`register_builtins`:

- `ls` — list directory entries (respect ignore patterns, sorted), `directory`
  arg, `recursive` optional.
- `find` — find files by glob/extension/name under a directory.
- `grep` — regex search within files, `pattern`, `path`, `include` globs,
  line context.

All read-only (`PermissionClass::Read`), and — critically — their presence must
**flip the §1.2 guideline** from "use bash for ls/rg/find" to "use the dedicated
tools", matching pi's conditional guideline emitter.

### 2.2 Align `edit` semantics with pi
pi's `edit` is a single `path` + `edits[]` (an array of `{oldText,newText}`
pairs) with uniqueness enforcement across the whole file in one call, and hints
to merge nearby changes into one edit. com's `edit` is one operation per call,
first-occurrence replace.

For drop-in behaviour, port the multi-edit shape:
- Change schema to accept an array of edits in a single call.
- Enforce oldText uniqueness (error listing the ambiguity) like pi, and expose
  the "merge nearby changes into one edit" guidance in the schema description.
- This also materially reduces the number of tool round-trips (helps §1's goal).

### 2.3 `edit-diff`
Implement pi's `edit-diff` (apply a unified diff to a file) or fold its
behaviour into `edit`. Table this behind §2.2 — only needed if users rely on
patch-based editing.

### 2.4 Image read
`read` should accept image paths and pass them as attachments to the provider
(pi sends jpg/png/gif/webp/bmp as attachments). Requires the provider request +
schema in `types.rs`/`llm.rs` to carry image content blocks. Lower priority:
depends on whether the target provider supports vision.

---

## 3. Priority 3: provider/model abstraction

com hardcodes a single `twobobs` HTTP gateway and one model line.
Pi's value here is the `pi-ai` multi-provider layer. For a drop-in in the
`COM_*`/`.com` world with extensions excluded, we need only internal
pluggability, not pi's extension-provider registry.

### 3.1 Provider trait with a registry
- Keep `LlmClient` as the seam; add a thin `Provider` abstraction that maps a
  `llm_provider` string to a client implementation (the trait already exists —
  this is an instantiation/router layer).
- Ship the existing `twobobs` HTTP gateway as provider `twobobs`. Add an
  `openai`-compatible provider (`OPENAI_BASE_URL`/`OPENAI_API_KEY`/model)
  covering OpenAI/Anthropic/Google-compatible endpoints via their OpenAI-
  compatible surfaces, so the same code base can target multiple backends
  without new dependencies.

### 3.2 Model selection parity
- `model` config + `/model` command exist already; extend to a **model
  registry** (list available models per provider) and `scoped-models`
  equivalents so `/model` can enumerate and switch.
- Surface model/usage/cost on the context view (partly present).

---

## 4. Priority 4: sessions & durability parity

com has a working `.com/sessions` jsonl repo with branching + compaction, but
pi's interactive flows are richer. Port the missing user-facing operations:

- `resume` — list/pick a prior session on startup (session picker in TUI).
- `name` — name/rename a session.
- `fork` — branch a session from a checkpoint.
- `export` / `import` — port pi's session exchange (export as JSON/Markdown,
  import a remote/exported session).
- `changelog` — human-readable log of a session's changes.
- `share` — optional; only if session sharing matters to the user.

Compatibility note: pi's durable backends are sqlite; com is jsonl. Since config
paths are out of scope, **jsonl stays** — the JSON event shape is what matters
for any `import`/`export` interop, so align the exported JSON schema with pi's
session event format to the degree needed for `export`/`import` round-tripping.

---

## 5. Priority 5: interactive / permission / misc parity

### 5.1 TUI behaviour
- `yes/no` and multi-select confirmation prompts (pi uses them for
  write/overwrite and trust decisions).
- Keybindings view + configurable bindings (`hotkeys`, keybindings.ts parity).
- Editor/view overlays beyond `context`/`full` where pi exposes them.

### 5.2 Permissions & project trust
- `PermissionClass` exists but is **never enforced**; `bash`/`write` always run.
- Implement a permission manager in the same style as pi's `trust-manager`:
  read-only ops free, mutation/`bash` gated on project trust, persistent trust
  decision in `.com/` (config paths stay ours).
- `/trust` command to view/set trust.

### 5.3 Auth management
- `login` / `logout` commands storing credentials in `.com/` (not pi's
  `~/.pi`), driving the provider (§3) credential lookup.
- Replace env-only key injection with a small credential store read by
  providers, with `*_API_KEY` env as fallback.

### 5.4 Operational modes
- `print` / non-interactive mode: com has a single-shot prompt mode; extend to
  accept piped/scripted input and `--model`/`--provider`/`--session` flags,
  mirroring pi's contract (`print-mode.ts`, `rpc`).
- RPC mode is lower priority; only if headless programmatic use is required.

### 5.5 Telemetry / diagnostics (optional)
- Lightweight usage/cost counters on the context view. Skip where not needed.

---

## 6. Cross-cutting: testability of "no speculative tool use"

§1's fix must not regress. Add a test harness seam:

- Teach `Agent`/`run_turn` to record tool-call counts (already traceable via
  history) so an integration test can assert "fact-answer prompt ⇒ zero tool
  calls" and "retrieval prompt ⇒ uses dedicated tool, not raw `bash`".
- Unit-test schema `description` fields for the §2.2/§2.3 guidance content.
- Keep the existing test suite green (it already asserts prompt/AGENTS.md
  loading and history behaviour).

---

## 7. Explicitly out of scope (per user)

- **pi extension SDK / plugin host** — com stays native.
- **pi config paths / env names** — keep `COM_*`, `.com/`, repo-root
  `AGENTS.md`/`SOUL.md`.
- **Dependencies** — all new crates must be Nix-managed, no npm/pip/cargo-install
  tooling; add any new workspace deps via `Cargo.toml` + nixpkgs.

---

## 8. Suggested sequencing

| Order | Work | Unblocks | Est. size |
|-------|------|----------|-----------|
| 1 | §1 tool-use fix (schemas + guidelines + test) | correctness pain | ✓ done |
| 2 | §2.1 grep/find/ls | tool parity | ✓ done |
| 3 | §2.2 edit multi-edit + uniqueness | round-trip reduction | ✓ done |
| 4 | §3 provider registry + openai-compatible | drop-in viability | ✓ done |
| 5 | §5.2 trust/permissions | safe mutation | ✗ drop |
| 6 | §4 sessions ops (name/changelog/export) | drop-in workflows | ✓ done |
| 7 | §5.3 auth store | provider utility | S |
| 8 | §5.1/§5.4 TUI/print/rpc parity | full drop-in | L |
| 9 | §2.4 image read, §2.3 edit-diff, §5.5 telemetry | polish | S |

**Committed target (confirmed by user): orders 1–5** — this IS the basic drop-in cut. Orders 6–9 (image read, edit-diff, telemetry, RPC) are follow-on polish and are NOT in the committed scope.

---

## 9. Definition of done (committed basic cut, orders 1–5)

A user can, from an arbitrary repo, run `com <prompt>` or launch the TUI and:

- get a terse, correct answer **without noise tool calls** on fact-only prompts;
- explore and edit the codebase via `read`/`write`/`edit`/`bash`/`grep`/`find`/
  `ls` with guidance that matches pi's;
- switch models/providers via `/model` + config, with credentials managed under
  `.com/`;
- get durable, resumable, forkable sessions and project-trust gating on
  mutations;
- run non-interactively for scripting.

Nothing in the above requires the pi extension host or pi config layout.
