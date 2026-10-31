# Downstream maintenance

| Item                    | Value                                      |
| ----------------------- | ------------------------------------------ |
| Fork / push remote      | `bhanutejags/herdr` / `origin`             |
| Upstream / fetch remote | `herdrdev/herdr` / `upstream`              |
| Downstream branch       | `feature/status-hover`                     |
| Initial upstream base   | `ee5bfb5041bb3e4a49d591c72308aae3c81ecdde` |
| Upstream mirror         | `master`, without downstream changes       |

## Patch contract

- Command status entries accept `output_format = "json"` with `text` and optional `tooltip` strings. The default remains plain text.
- Text supports Unicode, emoji, and Nerd Font glyphs. Nerd Font glyphs require a matching terminal font. GIFs, images, and terminal escape sequences are not supported in these fields.
- Hover details come from cached command results; hovering does not run a command or fetch data.
- Hover cards have a 150 ms text/border opening fade, fixed geometry, and no animation repaint scheduling after completion.
- The optional `endpoint.status-details.v1` JSON companion carries tooltips. Frozen endpoint codecs, wire tags, fixtures, and generation-1 contracts remain unchanged.
- The weather producer belongs to the separate dotfiles repository: `src/dot_local/bin/executable_wttr`. It is not bundled here.

The downstream commits modify command parsing, status-detail projection, client hover rendering/input, tests, and unreleased configuration documentation. Keep upstream license and attribution files intact.

## Update a candidate

Use Jujutsu. Do not rebase the published branch in place or push to `upstream`.

1. Start from a clean working copy. Inspect `jj status` and record the full published `feature/status-hover@origin` commit after fetching `origin`.
2. Create an isolated candidate workspace, then duplicate the downstream stack onto the fetched upstream revision. Copy the duplicated tip ID from the command output; do not select the old published tip.

```sh
jj git fetch --remote origin
jj git fetch --remote upstream --branch master
jj log -r feature/status-hover@origin --no-graph -T 'commit_id ++ "\n"'
jj workspace add <candidate-dir> -r feature/status-hover
cd <candidate-dir>
jj duplicate 'master@upstream..feature/status-hover' --onto master@upstream
jj edit <duplicated-tip>
jj resolve --list
jj diff --from master@upstream
```

Resolve conflicts only in the candidate. Preserve plain-text compatibility, bounded JSON parsing, visible-segment indexing, hover dismissal/occlusion, stationary-clock retention, and finite animation scheduling. Do not change frozen protocol expectations to make a check pass. If upstream already implements the contract, verify that behavior and obtain approval before retiring the downstream patch.

Run fetched source/build scripts only in a disposable runner or an approved sandbox. Native checks require the pinned Rust toolchain, Zig 0.16.0, `just`, `cargo-nextest`, and Bun; fork CI uses Bun 1.4.4.

```sh
filter='test(tab_bar) | test(json_status) | test(status_hover) | test(status_card) | test(status_details) | test(client::endpoint::control) | test(server::client_shell) | test(protocol::)'
just ci "$filter"
just docs-contract-test
```

These are focused native checks, not a release qualification or Windows cross-check. Run `just check` with the required Windows SDK before a release; SDK license acceptance is a human action. Run `just bench-render-scale` when changing pane-scaled paths, and investigate failures rather than weakening assertions.

## Publish a candidate and promote

The fork-only workflow `.github/workflows/fork-hover.yml` runs Linux/macOS checks on pushes to `feature/status-hover` and `fork-update/**`. It does not publish releases, install binaries, or run on a schedule.

```sh
jj bookmark create fork-update/<name> -r @
jj git push --remote origin --bookmark fork-update/<name>
gh run list --repo bhanutejags/herdr --branch fork-update/<name>
gh run watch <run-id> --repo bhanutejags/herdr --exit-status
```

Inspect the exact candidate diff and wait for both OS checks. Before promotion, fetch `origin` and verify its published feature tip still equals the commit recorded before the update. If it changed, stop and rebuild the candidate against the new tip.

After approval, while editing the validated candidate tip:

```sh
jj git fetch --remote origin
jj bookmark create backup/status-hover-<name> -r feature/status-hover@origin
jj bookmark move feature/status-hover --to @ --allow-backwards
jj git push --remote origin --bookmark feature/status-hover
jj new
```

Jujutsu checks the remote against its last fetched state when pushing. Keep the backup until the promoted build is verified. Do not advance `master` as part of this procedure: inherited master workflows include upstream publication/integration jobs. Do not open an upstream implementation PR.

## Build and install separately

Use `just build` in the approved build environment, or dispatch the inherited `build-artifacts-manual.yml` workflow against `feature/status-hover`. Select the required platform; use `ReleaseFast` and SIMD `true` to match the native preview settings. Its downloadable artifacts expire after seven days and include commit/target/SHA-256 metadata.

The inherited `herdr update` command reads upstream distribution manifests, not this fork. Do not use it for a patched binary; rebuild or download the approved fork revision instead. Installation and restarting a live server require separate approval. Keep preview sessions/configuration separate from the main session.
