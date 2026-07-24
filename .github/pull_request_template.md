## Summary

- 

## Verification

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace`
- [ ] WinDivert diagnostics if backend code changed
- [ ] Real dual-NIC test note if packet routing changed

## Risk

- [ ] No HTTPS interception or root certificate behavior added
- [ ] No service/updater routing loop introduced
- [ ] Matched traffic still fails closed when target adapter is unavailable
