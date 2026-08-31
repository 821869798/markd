# Task 5 Report: Shell Initialization Code Generation

## RED

Command:

```text
cargo test shell::tests --no-fail-fast && cargo test --test shell_setup --no-fail-fast
```

Result: failed during compilation before implementation. The new integration test reported:

```text
error[E0432]: unresolved import `mkd::shell`
 --> tests\shell_setup.rs:2:10
  |
2 | use mkd::shell::{Shell, init_script};
  |          ^^^^^ could not find `shell` in `mkd`
```

## GREEN

Focused validation:

```text
cargo test shell::tests --no-fail-fast
```

Result: passed. The filtered command completed successfully (0 matching tests; all test targets compiled).

```text
cargo test --test shell_setup --no-fail-fast
```

Result: passed, 8 passed, 0 failed.

The integration tests cover Bash, Zsh, Fish, and PowerShell script structure, argument forwarding, hidden `__select` routing, empty/failed selection guards, literal path handling, no unsafe `eval`, and init stdout/stderr behavior.

Full validation:

```text
cargo fmt --all -- --check
```

Result: passed.

```text
cargo clippy --all-targets --all-features -- -D warnings
```

Result: passed.

```text
cargo test --all-targets --all-features --no-fail-fast
```

Result: passed: 27 unit tests, 11 CLI integration tests, and 8 Shell integration tests; 0 failed.

Interpreter checks:

```text
bash -n <generated-init.bash>
```

Result: passed.

```text
zsh -n <generated-init.zsh>
fish -n <generated-init.fish>
```

Result: skipped because `zsh` and `fish` are not installed on this machine.

```text
pwsh -NoProfile -Command '<PowerShell parser check>'
```

Result: passed with no parser errors.

## Implementation Notes

- `mkd init <shell>` writes only the generated script to stdout and does not write diagnostics to stderr.
- The hidden clap command is explicitly named `__select`; its existing unimplemented behavior remains unchanged.
- Bash/Zsh use `command mkd` and `builtin cd --`.
- Fish uses `command mkd`, immediately saves `$status`, and uses a quoted command substitution so multiline output remains one path value.
- PowerShell resolves an executable with `Get-Command mkd -CommandType Application` and uses `Set-Location -LiteralPath`.
