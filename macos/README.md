# macOS draft selection and CanvasTTY

Based on upstream tag `rust-v0.159.2`, commit `ff6aec96948b70d94983af2641a6b67c94faeff5`.
This branch adds a separate `canvastty-codex-tui` frontend. It connects to the
installed official **codex-cli 0.159.2** over a private local Unix socket.
The ordinary upstream TUI entry point and backend/core/protocol/CLI sources are
unchanged. The launcher rejects other backend versions explicitly.

## Behavior

- ⌘A selects the entire draft, including multiline text outside the viewport.
  Typing, paste and Backspace replace the selection.
- ⌘C copies the selected draft. Large pasted text is expanded before copying,
  so the clipboard receives the text instead of a `[Pasted Content …]` marker.
- The CanvasTTY patch routes ⌘V text and images to the draft and keeps native
  editable fields working. F2 and Home reach the focused terminal application;
  the existing canvas actions remain available outside terminal/editable focus.
- Existing approval/sandbox flags, config overrides and explicit resume controls
  are preserved. Each launcher owns and cleans up only its backend process group.
- The launcher accepts interactive flags and resume UUIDs, but rejects positional
  initial prompts and CLI image arguments, which can automatically submit a turn.
  Pasting images interactively remains supported.

## Build and launch

Requirements: macOS, the repository's Rust 1.95.0 toolchain, Node.js, and the
official Codex 0.159.2 executable. See the upstream build prerequisites.

From this repository:

```sh
cd codex-rs
CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked -p codex-tui --bin canvastty-codex-tui
cd ..
node macos/codex-tui-launch.mjs \
  --backend /absolute/path/to/official/codex \
  --frontend "$PWD/codex-rs/target/debug/canvastty-codex-tui" --
```

The frontend expects terminal support for Super key events. The included
CanvasTTY integration supplies these events. The official executable stays
installed separately; the launcher does not replace it or write global config.

`Cargo.lock` normalizes the 159 local workspace package versions to the release
version 0.159.2. All 1,313 external dependency blocks match the upstream lock.

## CanvasTTY 1.7.0 integration

Place clean CanvasTTY 1.7.0 sources and this checkout next to one another:

```text
workspace/
  CanvasTTY/
  codex-macos-tui/
```

Then, from `CanvasTTY/`:

```sh
git apply --check ../codex-macos-tui/macos/canvastty/CanvasTTY-v1.7.0.patch
git apply ../codex-macos-tui/macos/canvastty/CanvasTTY-v1.7.0.patch
npm ci
npm run build
npx electron-builder --dir --mac --arm64 --publish never \
  -c ../codex-macos-tui/macos/canvastty/electron-builder.yml
```

The resulting app contains the frontend, launcher and license in
`Contents/Resources/codex-native-tui/`. The normal macOS app identity and user
data remain unchanged. The adapter resolves resources from the installed bundle,
so moving the app does not depend on the checkout path. Missing partial resources
produce an explicit launch error. Without either resource, upstream launch is
preserved. The patch also contains the previously verified per-launch browser
capability lifetime and clipboard lifecycle fixes. Its file hashes are recorded
in `CanvasTTY-v1.7.0-files.json`; CanvasTTY-derived code is covered by the adjacent
MIT license.

This packaging recipe was verified on Apple Silicon. Intel packaging has not been
verified. Keep a backup of the installed app before replacing it. An upstream
CanvasTTY update can replace this custom bundle; rebase, rebuild and verify it
before updating. For another Codex backend version, port and verify the frontend
and change the launcher's version requirement together.

## Validation

```sh
node --test macos/codex-tui-launch.test.mjs
cd codex-rs
just test -p codex-tui --locked -E 'test(select_all_tests) | test(owned_local_resume) | binary(canvastty-codex-tui)'
just fmt
just fix -p codex-tui --locked
```

The focused TUI tests passed 21/21; launcher tests passed 14/14. GUI checks in
CanvasTTY covered UTF-8/emoji, multiline and offscreen selection, large paste
copy, replacement/deletion, image paste, F2, Home, native search fields and a
resume picker kept open for 90 seconds. Drafts were not submitted.

The full TUI suite is **not entirely green** in the local macOS environment.
With upstream fixture version 0.0.0, it reported 5,590 passed, three failed and
eight ignored; one reported pass exited at an existing network-disabled guard.
The three failures were also reproduced with clean upstream TUI in the same
environment: two number-format snapshots (`8,000` vs `8 000`) and a nested
Seatbelt sandbox denied by the outer sandbox. Version-sensitive upstream fixtures
also fail when tested with release version 0.159.2. Rust formatting and scoped
Clippy passed; repository-wide formatting remained incomplete because DotSlash
was unavailable and Python formatters lacked cache access. Bazel lock regeneration
was unavailable locally. No backend request or existing-session GUI resume was tested.
