# Merge Prep: redesign/v2 → main

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Prepare `redesign/v2` for a clean squash-merge into `main`.

**Architecture:** 7 independent tasks across edition upgrade, CI workflows, release config, docs cleanup, and queue hygiene. Finish with a single verification pass.

**Tech Stack:** Rust 2024 edition, GitHub Actions, release-please, mdbook, cargo publish

---

## Context & Decisions

### Edition 2024 + resolver 3
- Rust 1.93.1 (stable) supports edition 2024.
- All `dyn` usages are already explicit (`Box<dyn>`, `Arc<dyn>`, `&dyn`) — no bare trait objects to fix.
- No `extern crate`, no `macro_use`, no unsafe blocks — edition migration is mechanical.
- 26 crates inherit `edition` from workspace; 8 have local `edition = "2021"` overrides that need updating.
- `rust-version` should be set to `"1.85"` (edition 2024 MSRV) in `[workspace.package]`.

### Workflows to restore
- `docs.yml` — mdbook deploy to GitHub Pages (copy from main as-is)
- `publish.yml` — manual cargo publish (update crate list for new workspace)
- `release-please.yml` — automated release PRs + publish (update crate list + config)

### Crate version strategy
- Main has `neuron`, `neuron-tool`, `neuron-context`, `neuron-mcp`, and 3 providers at v0.3.0 on crates.io.
- Branch has these same crate names at v0.1.0 — **completely different API surface**.
- **Decision:** Bump all to `1.0.0-alpha.1`. This is a ground-up redesign, not a patch to v0.3. The `1.0.0-alpha` prefix signals "new major, pre-release" and avoids any semver confusion with the published 0.3.x line. Alternatively, use `0.4.0` if you want to stay pre-1.0 — but the API has zero backward compatibility with 0.3, so a major bump is cleaner.
- New crates (`layer0`, `neuron-turn`, `neuron-hooks`, `neuron-orch-*`, `neuron-state-*`, `neuron-env-local`, `neuron-secret-*`, `neuron-auth-*`, `neuron-crypto-*`, `neuron-hook-security`) start at `0.1.0`.

### Rebase strategy
- **Squash merge.** The 105 commits include build-then-remove churn (brain crate: 20 commits added, 1 commit removed). Interactive rebase to clean logical groups would be 30+ picks with conflict resolution — not worth it.
- Squash-merge preserves all history on the branch (branch stays) and gives main a single clean commit.
- PR description should contain the structured summary of what changed.

### Docs assessment
- **mdbook:** Solid — 2,257 lines, 14 chapters covering all 6 layers, builds clean. Ready for merge.
- **llms.txt:** Good — accurate crate map, reading order, all 14 specs linked. One issue: links to `docs/architecture/*.md` are internal design docs (HANDOFF, decision map) — fine for agent consumption but check if these should be in the book too.
- **Stale docs:** `NEURON-REDESIGN-PLAN.md` references old crate names (`neuron-loop`, `neuron-types`) as historical design context — acceptable, but add a deprecation header.
- **Missing:** No per-crate README for the new crates (`layer0`, `neuron-turn`, `neuron-hooks`, etc.) — these inherit `description` from Cargo.toml but have no README.md. Not blocking for merge, but needed before crates.io publish.

### ralph_queue.md
- Completed section references deleted specs (14–19) and deleted brain crate — clean up to only reference existing specs.

---

## Tasks

### Task 1: Edition 2024 + resolver 3

**Files:**
- Modify: `Cargo.toml` (root)
- Modify: `layer0/Cargo.toml`
- Modify: `neuron-state-memory/Cargo.toml`
- Modify: `neuron-state-fs/Cargo.toml`
- Modify: `neuron-env-local/Cargo.toml`
- Modify: `neuron-orch-kit/Cargo.toml`
- Modify: `neuron-orch-local/Cargo.toml`
- Modify: `neuron-hooks/Cargo.toml`

**Step 1: Update root Cargo.toml**

Change workspace:
```toml
edition = "2024"
```
and:
```toml
resolver = "3"
```
and root `[package]`:
```toml
edition = "2024"
```
Add to `[workspace.package]`:
```toml
rust-version = "1.85"
```

**Step 2: Update 8 crates with local edition overrides**

In each of `layer0`, `neuron-state-memory`, `neuron-state-fs`, `neuron-env-local`, `neuron-orch-kit`, `neuron-orch-local`, `neuron-hooks` — change `edition = "2021"` → `edition = "2024"`.

**Step 3: Build and test**

```bash
nix develop -c cargo test --workspace --all-targets
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
```

If edition 2024 introduces new lints (like `tail_expr_drop_order`), fix them.

**Step 4: Format**

```bash
nix develop -c nix fmt
```

**Step 5: Commit**

```bash
git add -A && git commit -m "chore: upgrade to Rust edition 2024 + resolver 3"
```

---

### Task 2: Restore docs.yml workflow

**Files:**
- Create: `.github/workflows/docs.yml`

**Step 1: Copy from main exactly**

```yaml
name: Deploy Docs

on:
  push:
    branches: [main]
    paths:
      - 'docs/book/**'
  workflow_dispatch:

permissions:
  pages: write
  id-token: write

jobs:
  deploy:
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install mdBook
        uses: taiki-e/install-action@mdbook
      - name: Build book
        run: mdbook build docs/book/
      - name: Setup Pages
        uses: actions/configure-pages@v4
      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: target/book/
      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

**Step 2: Commit**

```bash
git add .github/workflows/docs.yml && git commit -m "ci: restore docs deployment workflow"
```

---

### Task 3: Restore publish.yml + release-please.yml with updated crate lists

**Files:**
- Create: `.github/workflows/publish.yml`
- Create: `.github/workflows/release-please.yml`
- Create: `release-please-config.json`
- Create: `.release-please-manifest.json`

**Step 1: Determine publish order**

Dependency order (leaves first, umbrella last):
```
layer0
neuron-secret
neuron-crypto
neuron-auth
neuron-turn
neuron-hooks
neuron-tool
neuron-context
neuron-state-memory
neuron-state-fs
neuron-secret-env
neuron-secret-vault
neuron-secret-aws
neuron-secret-gcp
neuron-secret-keystore
neuron-secret-k8s
neuron-auth-static
neuron-auth-file
neuron-auth-oidc
neuron-auth-k8s
neuron-crypto-vault
neuron-crypto-hardware
neuron-hook-security
neuron-op-react
neuron-op-single-shot
neuron-provider-anthropic
neuron-provider-openai
neuron-provider-ollama
neuron-mcp
neuron-env-local
neuron-orch-kit
neuron-orch-local
neuron
```

**Step 2: Create publish.yml** — same structure as main but with updated CRATES array.

**Step 3: Create release-please.yml** — same structure as main but with updated CRATES array.

**Step 4: Create release-please-config.json** — register all 33 workspace members.

**Step 5: Create .release-please-manifest.json** — all crates at `0.1.0` (initial).

**Step 6: Commit**

```bash
git add .github/workflows/publish.yml .github/workflows/release-please.yml \
  release-please-config.json .release-please-manifest.json
git commit -m "ci: restore publish and release-please workflows for v2 workspace"
```

---

### Task 4: Clean up ralph_queue.md

**Files:**
- Modify: `ralph_queue.md`

**Step 1: Remove completed brain items that reference deleted specs**

Remove all completed items referencing `specs/14-*` through `specs/19-*`. Keep the non-brain completed items.

**Step 2: Commit**

```bash
git add ralph_queue.md && git commit -m "chore: clean ralph_queue of deleted brain references"
```

---

### Task 5: Add deprecation header to NEURON-REDESIGN-PLAN.md

**Files:**
- Modify: `NEURON-REDESIGN-PLAN.md`

**Step 1: Add header**

```markdown
> **⚠️ Historical document.** This was the original redesign plan. The redesign is now implemented.
> Crate names referenced here (neuron-types, neuron-loop, neuron-runtime, neuron-otel) no longer
> exist in the workspace. For current architecture, see `SPECS.md` and `docs/book/`.
```

**Step 2: Commit**

```bash
git add NEURON-REDESIGN-PLAN.md && git commit -m "docs: mark redesign plan as historical"
```

---

### Task 6: Update llms.txt reading order

**Files:**
- Modify: `llms.txt`

**Step 1: Review and update**

The reading order currently starts with the redesign plan (now historical). Update to point to the book and specs first:

```
## Reading Order

1. [Specs Index](SPECS.md)
2. [Architecture: 6-Layer Model](docs/book/src/architecture/layers.md)
3. [Architecture: Protocol Traits](docs/book/src/architecture/protocol-traits.md)
4. [Crate Map](docs/book/src/reference/crate-map.md)
5. [Redesign Plan (historical)](NEURON-REDESIGN-PLAN.md)
```

**Step 2: Commit**

```bash
git add llms.txt && git commit -m "docs: update llms.txt reading order for current architecture"
```

---

### Task 7: Final verification

**Step 1: Full test + lint + format + doc**

```bash
nix develop -c cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
nix develop -c nix fmt
cargo doc --workspace --no-deps
mdbook build docs/book
```

**Step 2: Verify no stale references**

```bash
# No brain references in code
grep -rn 'brain' --include='*.rs' --include='*.toml' | grep -v target/
# No references to deleted specs in non-historical files
grep -rn 'specs/1[4-9]\|specs/2' --include='*.md' | grep -v target/ | grep -v ralph_queue | grep -v DEVELOPMENT-LOG | grep -v plans/
```

**Step 3: Verify git status clean**

```bash
git status
```

---

## Post-Merge Strategy

### Squash Merge Command
```bash
git checkout main
git merge --squash redesign/v2
git commit  # write structured PR summary as commit message
```

### Crates.io Migration
The 7 crates that share names with published v0.3.0 (`neuron`, `neuron-tool`, `neuron-context`, `neuron-mcp`, `neuron-provider-{anthropic,openai,ollama}`) need a version bump past 0.3.0. Options:

1. **`1.0.0-alpha.1`** — signals "new major, pre-release." Cleanest semver story.
2. **`0.4.0`** — stays pre-1.0 but the API is 100% different from 0.3.
3. **`0.90.0`** — convention for "approaching 1.0" pre-releases.

Recommendation: go with option 1 or 2 based on comfort. Either way, do NOT publish at `0.1.0` — that's lower than the already-published `0.3.0` and crates.io will reject it.

New crates (26 of them) can publish at `0.1.0` fine.

### Missing Before First Publish
- Per-crate `README.md` for all new crates (crates.io displays it)
- Per-crate `description` field in Cargo.toml (required by crates.io)
- `CHANGELOG.md` per crate (release-please generates these)
- Decide on `categories` and `keywords` in Cargo.toml
