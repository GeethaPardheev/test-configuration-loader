# Design Document — Unified Configuration Loader

## 1. Overview

This document explains the architecture, design decisions, and trade-offs of
the `configuration-loader` crate.

## 2. Architecture

The crate is split into focused, single-responsibility modules:

```
configuration-loader/
├── src/
│   ├── lib.rs          — crate root, re-exports, integration tests
│   ├── partial.rs      — PartialConfig (Option<T> fields) + LogLevel
│   ├── defaults.rs     — hard-coded default values
│   ├── env.rs          — environment variable reader
│   ├── file.rs         — config file loader (TOML + YAML)
│   ├── merge.rs        — pure merge function
│   ├── validate.rs     — Config struct + validation
│   ├── config.rs       — public Config::load() entry points
│   ├── error.rs        — ConfigError enum
│   └── hot_reload.rs   — (bonus) file-watcher hot-reload
```

### Data flow

```
defaults::defaults()  →──────────────┐
                                      ↓
file::from_file()     →──── merge ───→ merge ───→ validate() ───→ Config
                                      ↑
env::from_env()       →──────────────┘
                        (highest priority)
```

## 3. Configuration Sources and Loading

### 3.1 Hard-coded defaults (`defaults.rs`)

Returns a `PartialConfig` with fields that have a universally safe starting
value.  `database_url` is intentionally `None` — there is no meaningful
default for a connection string, so the loader will fail with a clear error
if neither the file nor the environment supplies one.

### 3.2 Config file (`file.rs`)

Resolution order (from highest to lowest specificity):

1. An explicit path passed to `Config::load_from(path)`.
2. The `APP_CONFIG_FILE` environment variable.
3. `config.toml` in the current working directory.
4. `config.yaml` / `config.yml` in the current working directory.

The file is **optional** in the default flow.  If no file is found, the
module returns an all-`None` `PartialConfig` (i.e. the file layer contributes
nothing, and lower-priority defaults remain in effect).

If an explicit path is supplied but the file does not exist, the error is
`ConfigError::FileNotFound` — this is a programmer mistake and must not be
silently ignored.

Supported formats are detected by file extension:

| Extension      | Parser              |
|----------------|---------------------|
| `.toml`        | `toml::from_str`    |
| `.yaml`, `.yml`| `serde_yaml::from_str` |
| anything else  | `ConfigError::UnsupportedFormat` |

Both parsers deserialise directly into `PartialConfig`, so any key absent from
the file becomes `None` — no default injection happens at this layer.

### 3.3 Environment variables (`env.rs`)

All variables use the `APP_` prefix:

| Variable              | Config field      | Type       |
|-----------------------|-------------------|------------|
| `APP_DATABASE_URL`    | `database_url`    | `String`   |
| `APP_PORT`            | `port`            | `u16`      |
| `APP_LOG_LEVEL`       | `log_level`       | `LogLevel` |
| `APP_MAX_CONNECTIONS` | `max_connections` | `u32`      |
| `APP_TIMEOUT_SECS`    | `timeout_secs`    | `u64`      |
| `APP_CONFIG_FILE`     | `config_file`     | `String`   |

A missing variable contributes `None`; a present-but-unparseable variable
returns `ConfigError::InvalidEnvVar` immediately with the variable name, the
bad value, and the parse error message.

## 4. Precedence Rules

```
Environment variables   (highest — always win)
  ↑
Config file             (mid — overrides code defaults)
  ↑
Hard-coded defaults     (lowest — baseline safety net)
```

This is enforced in `merge.rs` using `Option::or`:

```rust
field: overlay.field.or(base.field)
```

A `None` in the overlay **never** clears a `Some` in the base.  This means:

- Setting `APP_PORT=9090` always overrides whatever the file says.
- A file that specifies only `port` does not affect `database_url`.
- Omitting a field from the file does not reset it to `None`.

## 5. Error Modeling and Reporting

All errors are variants of the `ConfigError` enum (defined in `error.rs`) and
derived via `thiserror`.  Every variant carries enough context to tell the
operator exactly what went wrong and how to fix it:

| Variant              | When it occurs                                      |
|----------------------|-----------------------------------------------------|
| `FileNotFound`       | Explicit file path given but file does not exist    |
| `ParseError`         | File exists but its syntax is invalid               |
| `InvalidEnvVar`      | Env var present but cannot be converted to `T`      |
| `MissingRequired`    | A required key is absent from all three sources     |
| `ValidationError`    | Value present but fails a domain rule (e.g. port=0) |
| `UnsupportedFormat`  | File extension is not `.toml`, `.yaml`, or `.yml`   |
| `Io`                 | Non-"not found" I/O error reading a file            |
| `WatcherError`       | Hot-reload watcher setup or runtime error           |

`MissingRequired` includes an `env_hint` showing which environment variable
to set, making it immediately actionable:

```
required configuration key `database_url` is missing — set it via
environment variable `APP_DATABASE_URL` or in the config file
```

## 6. Validation (`validate.rs`)

The `validate` function is the single gateway between `PartialConfig` and
`Config`.  It:

1. **Unwraps** every required field, returning `ConfigError::MissingRequired`
   if any are absent.
2. **Checks domain invariants** (e.g. `port != 0`, `max_connections >= 1`,
   `timeout_secs >= 1`, `database_url` is non-empty).
3. **Constructs** the final `Config` only when all checks pass.

Key design principle: **partial configuration is never silently accepted**.
If even one required field is missing or invalid, the entire load fails with
a clear error.  The caller is never handed a `Config` that might crash the
application at runtime.

## 7. Testability

The design was explicitly optimised for easy, hermetic testing:

- `Config::from_partial(partial)` allows tests to construct any
  configuration directly from a `PartialConfig` without touching the
  filesystem or the process environment.
- `Config::load_from(path)` loads a specific file, allowing tests to use
  `tempfile::NamedTempFile` with deterministic content.
- Individual modules (`merge`, `validate`, `defaults`) are pure or
  near-pure functions that can be tested in isolation.
- The env module provides a `with_env` helper (in its `#[cfg(test)]` section)
  that sets a variable, runs a closure, then restores the previous value —
  keeping tests hermetic without a third-party `test-env` crate.
- No `unwrap` or `expect` anywhere in production code — tests use `.expect`
  only where a failure would indicate a bug in the test itself.

## 8. Trade-offs

### Flat vs. nested configuration

`Config` uses a flat struct rather than nested sections (e.g. `db.url`,
`server.port`).  This is simpler to deserialise across both TOML and YAML and
avoids the complexity of merging nested `Option<struct>` trees.  Large
applications would benefit from grouping (e.g. `DatabaseConfig`,
`ServerConfig`), but that is outside the scope of this task.

### `PartialConfig` duplication

Maintaining both `PartialConfig` and `Config` means every field is declared
twice.  The alternative — a single struct with `Option` fields and a `build()`
method — was rejected because it leaks `Option<T>` into the caller's API,
making usage awkward.  The explicit two-struct pattern keeps the public API
clean (`cfg.port`, not `cfg.port.unwrap()`).

### `serde` for all three sources

Using `serde::Deserialize` for both TOML and YAML keeps the deserialization
code trivially small and consistent.  Environment variables cannot be
deserialised via `serde` directly (they have no document structure), so they
are parsed field-by-field with `str::parse::<T>()` — but the same
`PartialConfig` type is used as the output, so the merge step is uniform.

### Hot-reload error handling

When a reload fails, the watcher silently retains the last-known-good config
rather than crashing or exposing an error to the caller.  In production this
would be paired with structured logging.  The intent is to avoid cascading
failures: a temporary parse error in the config file during a live edit should
not take down a running service.

### `notify` crate choice

`notify` is the de-facto standard cross-platform file-watcher for Rust.  It
uses native OS APIs (FSEvents on macOS, inotify on Linux, ReadDirectoryChanges
on Windows) and falls back to polling on unsupported platforms.

## 9. How to Run Tests

```bash
# Format check
cargo fmt --check

# Lint (zero warnings allowed)
cargo clippy -- -D warnings

# Unit + integration tests (run sequentially due to env-var mutation)
cargo test -- --test-threads=1

# Or use the project alias
cargo test-seq

# Run the basic example (requires APP_DATABASE_URL to be set or a config file)
APP_DATABASE_URL=postgres://localhost/demo cargo run --example basic
```
