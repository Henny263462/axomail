## What

<!-- What does this PR change, and why? Link related issues. -->

## Checklist

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked` pass
- [ ] `pnpm typecheck`, `pnpm lint`, `pnpm test`, and `pnpm build` pass
- [ ] `pnpm check:file-sizes` passes (300-line guideline, 400 hard limit)
- [ ] New logic ships with tests; no real private mail accounts used
- [ ] No secrets in code, logs, errors, or fixtures
- [ ] Significant architecture decisions documented as an ADR in `docs/adr/`
- [ ] Documentation updated where behavior changed
