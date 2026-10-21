# Codebase Cleanup, Simplification, Performance & Security Tasks

This document tracks all identified code-quality improvements, simplifications, performance bottlenecks, and security hardening opportunities for `mdpkg`.

## Core Constraints & Principles
- **Zero Behavioral Changes**: All CLI arguments, flags, markdown formatting conventions, hook lifecycle stages, environment variables, and synchronization behaviors must remain 100% backward-compatible.
- **Evidence Before Completion**: Every task must be verified with `cargo test` and `cargo clippy --all-targets -- -D warnings`.
- **Modular & Incremental**: Each task is designed to be executable in isolation without cascading refactoring.

---

## Task Summary & Progress

| ID | Title | Target File | Area | Priority | Status |
|---|---|---|---|---|---|
| `CLN-01` | Fix Clippy redundant branch warning | `src/config.rs` | Cleanup | P0 | `[x]` |
| `CLN-02` | Fix Clippy collapsible `if` and `format!` warnings | `src/markdown.rs` | Cleanup | P0 | `[ ]` |
| `CLN-03` | Fix Clippy collapsible `if` warning | `src/reconcile.rs` | Cleanup | P0 | `[ ]` |
| `SEC-01` | Fix async-signal safety violation in signal handler | `src/privilege.rs` | Security | P0 | `[ ]` |
| `SEC-02` | Add dangling Polkit session rule cleanup on startup | `src/privilege.rs` | Security | P1 | `[ ]` |
| `SEC-03` | Replace keep-alive busy-sleep loop with condition variable / channel | `src/privilege.rs` | Performance / Safety | P1 | `[ ]` |
| `SEC-04` | Streamline privilege session username lookup | `src/privilege.rs` | Robustness | P2 | `[ ]` |
| `SIM-01` | Deduplicate catalog/host reconciliation in commands | `src/commands.rs` | Simplification | P1 | `[ ]` |
| `SIM-02` | Implement standard Rust conversion traits (`Display`, `FromStr`) | `src/model.rs` | Idiomatic Rust | P2 | `[ ]` |
| `SIM-03` | Clean up path normalization and tilde expansion | `src/config.rs` | Simplification | P2 | `[ ]` |
| `PERF-01` | Replace string-rendering diff with structural equality | `src/reconcile.rs` | Performance | P1 | `[ ]` |
| `PERF-02` | Add fast-path prefix filters to avoid unnecessary Regex matching | `src/markdown.rs` | Performance | P1 | `[ ]` |
| `PERF-03` | Eliminate temporary allocations in `render_document` | `src/markdown.rs` | Performance | P2 | `[ ]` |
| `PERF-04` | Reduce redundant string clones in reconciliation | `src/reconcile.rs` | Performance | P2 | `[ ]` |
| `TST-01` | Verify test suite and enforce zero warnings in CI/checks | Repository-wide | Quality Gate | P0 | `[ ]` |

---

## Detailed Task Breakdown

### Phase 1: Compiler & Linter Cleanups (P0)

#### [x] Task `CLN-01`: Fix Clippy redundant branch in `config.rs`
- **Location**: `src/config.rs:88-92`
- **Issue**:
  ```rust
  } else if cwd.join("packages.md").exists() {
      cwd.to_path_buf()
  } else {
      cwd.to_path_buf()
  };
  ```
  Both the `else if` condition and the `else` block evaluate to `cwd.to_path_buf()`, generating `clippy::if_same_then_else`.
- **Action**: Remove the redundant `else if cwd.join("packages.md").exists()` branch and keep the single `else { cwd.to_path_buf() }`.
- **Verification**: `cargo clippy --all-targets` produces no warning for `config.rs`.

#### [ ] Task `CLN-02`: Fix Clippy collapsible `if` and nested `format!` warnings in `markdown.rs`
- **Location**: `src/markdown.rs:84-89`, `src/markdown.rs:282-290`, `src/markdown.rs:326-335`
- **Issues**:
  1. Lines 84-89: Nested `if doc.title.is_none() && current_section.is_none()` under `if trimmed.starts_with("# ") ...` can be collapsed into a single condition (`clippy::collapsible_if`).
  2. Lines 282-290: Nested `if !out.ends_with("\n\n") && !out.is_empty()` under `if s_idx > 0 ...` can be collapsed.
  3. Lines 326 & 328: `format!("`{}`", item.name)` inside an outer `format!` call generates `clippy::format_in_format_args`.
- **Action**:
  - Combine nested `if` statements.
  - Simplify format strings to inline the backticks directly (e.g. `"- {} `{}` — {}\n"` or `format_args!`).
- **Verification**: `cargo clippy --all-targets` produces no warning for `markdown.rs`.

#### [ ] Task `CLN-03`: Fix Clippy collapsible `if` warning in `reconcile.rs`
- **Location**: `src/reconcile.rs:217-221`
- **Issue**: `else if is_inst { if seen_remove.insert(item.name.clone()) { ... } }` generates `clippy::collapsible_if`.
- **Action**: Collapse into `else if is_inst && seen_remove.insert(item.name.clone()) { ... }`.
- **Verification**: `cargo clippy --all-targets` produces no warning for `reconcile.rs`.

---

### Phase 2: Security & Privilege Session Hardening (P0 / P1)

#### [ ] Task `SEC-01`: Fix Async-Signal-Safety violation in signal handlers
- **Location**: `src/privilege.rs:28-43`
- **Issue**:
  - `sigint_handler` invokes `cleanup_polkit_rule()`, which calls `Command::new("sudo").args(["rm", "-f", POLKIT_RULE_PATH]).status()`.
  - In POSIX systems, `Command::new` performs memory allocations, mutex locking, and `fork`/`exec`. Calling non-async-signal-safe functions inside a signal handler leads to undefined behavior, deadlocks, and corrupted process state if interrupted while holding an internal libc or allocator lock.
- **Action**:
  - Replace POSIX C signal handler with a safe signal handling pattern (e.g. using a global atomic flag or `signal_hook` / standard Rust shutdown hook), OR write a minimal async-signal-safe routine that avoids heap allocations.
  - Ensure cleanup is reliably triggered on `Drop` and standard process termination.
- **Verification**: Verify `cargo test` and manual termination (`SIGINT` during sync) exits cleanly without hangs.

#### [ ] Task `SEC-02`: Clean up dangling Polkit session rules on startup
- **Location**: `src/privilege.rs` & `src/commands.rs`
- **Issue**:
  - If `mdpkg` is killed via uncatchable `SIGKILL` (`kill -9`) or system power failure, `/etc/polkit-1/rules.d/90-mdpkg-session.rules` remains on disk.
  - This leaves passwordless `org.manjaro.pamac.commit` access permanently enabled for that user.
- **Action**:
  - When acquiring privileges or during initial command startup, check if a stale `/etc/polkit-1/rules.d/90-mdpkg-session.rules` exists from a previous aborted run and ensure it is cleaned up or properly re-managed.
- **Verification**: Unit test / simulated stale rule check.

#### [ ] Task `SEC-03`: Replace keep-alive busy-sleep loop with condition variable / channel
- **Location**: `src/privilege.rs:86-100`
- **Issue**:
  - The background thread runs a loop with 500 iterations of `thread::sleep(Duration::from_millis(100))` to check an atomic boolean before running `sudo -n -v`.
  - This wakes up the CPU 10 times every second needlessly when idle.
- **Action**:
  - Replace the 100ms polling loop with a channel with timeout (`std::sync::mpsc::channel` with `recv_timeout(Duration::from_secs(50))`) or `std::sync::Condvar`.
  - On `Drop`, send a shutdown message to immediately wake up and join the thread without waiting.
- **Verification**: Background thread exits instantly upon drop without CPU polling wakeups.

#### [ ] Task `SEC-04`: Streamline privilege session username resolution
- **Location**: `src/privilege.rs:60-65`
- **Issue**:
  - Executes external command `id -un` via `Command::new("id")`.
  - Redundant subprocess invocation when the username can be read from standard environment variables (`$USER`, `$LOGNAME`) or `libc::getpwuid(libc::getuid())`.
- **Action**:
  - Resolve username via `env::var("USER")` or libc user lookup, falling back to `id -un` only if unavailable.
- **Verification**: `test_polkit_rule_content` and integration tests pass.

---

### Phase 3: Code Simplification & Deduplication (P1 / P2)

#### [ ] Task `SIM-01`: Deduplicate catalog/host loading and reconciliation in commands
- **Location**: `src/commands.rs:310-585`
- **Issue**:
  - `handle_sync`, `handle_add`, `handle_enable`, `handle_disable`, and `handle_remove` duplicate the same 20-30 lines:
    1. `ensure_catalog_exists(paths)`
    2. `fs::read_to_string(&paths.catalog_path)`
    3. `parse_document(&paths.catalog_path, ...)`
    4. `backend.query_installed()?`
    5. Load and parse `paths.host_path` if it exists
    6. `reconcile_catalog_to_host(...)`
    7. Render and write updated `host_doc` back to disk
- **Action**:
  - Extract a helper function:
    ```rust
    struct HostContext {
        catalog_doc: Document,
        installed: InstalledPackages,
        host_doc: Document,
    }
    fn load_host_context(paths: &ResolvedPaths, backend: &dyn SystemBackend) -> Result<HostContext, anyhow::Error>;
    fn save_host_document(paths: &ResolvedPaths, doc: &Document) -> Result<(), anyhow::Error>;
    ```
  - Refactor handlers to use these unified helpers, reducing ~120-150 lines of duplicate code.
- **Verification**: All integration tests in `tests/cli_integration.rs` pass without change.

#### [ ] Task `SIM-02`: Implement standard Rust conversion traits for domain models
- **Location**: `src/model.rs`
- **Issue**:
  - `PackageSource::prefix_str(&self) -> &'static str`
  - `PackageSource::from_prefix(prefix: Option<&str>, legacy_aur: bool) -> Result<Self, String>`
  - `HookStage::as_str(&self) -> &'static str`
  - `HookStage::from_stage_str(s: &str) -> Option<Self>`
  - Uses ad-hoc naming instead of standard traits (`Display`, `FromStr`, `TryFrom`).
- **Action**:
  - Implement `std::fmt::Display` for `PackageSource` and `HookStage`.
  - Implement `std::str::FromStr` / `TryFrom<&str>` for `HookStage`.
  - Keep backward-compatible helper methods as aliases or deprecate cleanly.
- **Verification**: Code compiles and `cargo test` passes.

#### [ ] Task `SIM-03`: Clean up path normalization in `config.rs`
- **Location**: `src/config.rs:36-57`
- **Issue**:
  - `normalize_data_dir` contains custom loop handling `Component::CurDir` and `Component::ParentDir`.
  - Can be clarified and tested for corner cases with standard path manipulation.
- **Action**:
  - Refactor path normalization into a clear, documented utility function.
- **Verification**: `config::tests` pass cleanly.

---

### Phase 4: Performance & Memory Optimizations (P1 / P2)

#### [ ] Task `PERF-01`: Replace markdown string serialization diff with structural equality
- **Location**: `src/reconcile.rs:139-143`
- **Issue**:
  ```rust
  let structure_changed = match existing_host {
      None => true,
      Some(host) => render_document(&host_doc) != render_document(host),
  };
  ```
  To check if the host document changed, both the newly reconciled document and the existing document are fully converted into multi-kilobyte strings and compared.
- **Action**:
  - Since `Document`, `SectionBlock`, `CategoryBlock`, and `PackageItem` derive `PartialEq, Eq`, compare `host_doc != *host` directly.
  - This avoids thousands of allocations and regex/format overhead on every sync or check.
- **Verification**: Run `cargo test`; verify `structure_changed` produces the identical boolean result in integration tests.

#### [ ] Task `PERF-02`: Add fast-path prefix filters to avoid unnecessary Regex matching
- **Location**: `src/markdown.rs:79-253`
- **Issue**:
  - `parse_document` iterates over every line and tests multiple complex regular expressions (`RE_SECTION`, `RE_CATEGORY`, `RE_ITEM`, `RE_POTENTIAL_ITEM`).
  - Most lines can be classified by inspecting the first 1-3 characters before invoking regex engines.
- **Action**:
  - Fast-check `trimmed.starts_with("##")`, `trimmed.starts_with("###")`, and `trimmed.starts_with('-')` before regex matching.
  - Skip non-matching lines immediately.
- **Verification**: `cargo test` passes with identical parse results.

#### [ ] Task `PERF-03`: Eliminate temporary allocations in `render_document`
- **Location**: `src/markdown.rs:269-341`
- **Issue**:
  - Repeated `out.push_str(&format!(...))` allocates a temporary `String` for every heading, note, and package item.
- **Action**:
  - Use `use std::fmt::Write;` and `write!(out, ...).unwrap();` directly to format into the pre-allocated `out` buffer.
  - Pre-allocate buffer capacity (`String::with_capacity(content_len_hint)`).
- **Verification**: `render_document` output is byte-for-byte identical.

#### [ ] Task `PERF-04`: Reduce redundant string cloning in reconciliation
- **Location**: `src/reconcile.rs:80-120`
- **Issue**:
  - In `reconcile_catalog_to_host`, `item.name.clone()` and `item.description.clone()` are cloned multiple times (into `catalog_packages`, `newly_added`, `reconciled_items`).
- **Action**:
  - Minimize cloning by borrowing where keys only need lookup, or taking ownership when transforming owned structures.
- **Verification**: Unit and integration tests pass without regression.

---

### Phase 5: Verification & Quality Gate (P0)

#### [ ] Task `TST-01`: Full verification suite
- Run `cargo fmt --check`
- Run `cargo clippy --all-targets -- -D warnings`
- Run `cargo test --all`
- Confirm all 15 unit tests and 8 integration tests pass with zero warnings and zero behavioral discrepancies.
