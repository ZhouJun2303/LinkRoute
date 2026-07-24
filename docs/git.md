# Git Reference

This repository is published to GitHub as `LinkRoute`.

Remote repository: `https://github.com/ZhouJun2303/LinkRoute.git`.

## First-Time Local Setup

```powershell
git init
git add .
git commit -m "Initial LinkRoute scaffold"
git branch -M main
git remote add origin https://github.com/ZhouJun2303/LinkRoute.git
git push -u origin main
```

## Normal Development Push

```powershell
git status -sb
git add README.md ROADMAP.md CHANGELOG.md docs crates scripts installer .github
git commit -m "feat: describe the change"
git push origin main
```

Before pushing, run:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

## Release Tags

Signed release tags trigger `.github/workflows/release.yml`:

```powershell
git tag -s v0.1.0 -m "LinkRoute v0.1.0"
git push origin v0.1.0
```

If GPG signing is not configured yet, use an annotated tag only for internal testing:

```powershell
git tag -a v0.1.0 -m "LinkRoute v0.1.0"
git push origin v0.1.0
```

## Files Not Committed

- `target/`
- `dist/`
- `installer/output/`
- `vendor/windivert/`

WinDivert binaries are fetched by script and packaged during release builds.