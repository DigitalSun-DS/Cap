# Building the GPUI desktop app quickly

This workspace is its own Cargo workspace (`apps/desktop-gpui/Cargo.toml`) and
its dependency tree is huge: GPUI, wgpu, ffmpeg, ONNX Runtime and whisper all
build here. The stock `release` profile is tuned for shipping
(`codegen-units = 1`, `lto = "thin"`), which makes it the wrong tool for the
edit/compile/test loop.

Use two loops instead.

## Inner loop: check, test, dev

```powershell
# Type-check only. No codegen, no link.
cargo check -p cap-desktop-gpui

# Unit tests for one module (harness links the whole bin, so filter).
powershell -NoProfile -File scripts/gpui-fast.ps1 -Mode test -Filter editor_effect_clipboard::tests

# Dev binary, already tuned for iteration (app crate at opt-level 0,
# incremental on, dependencies at opt-level 2).
cargo run -p cap-desktop-gpui
```

`scripts/gpui-fast.ps1` puts the ffmpeg, ONNX Runtime and DXC DLL directories
on `PATH`; the test harness fails with `STATUS_DLL_NOT_FOUND` without them. The
script also sets `CARGO_PROFILE_TEST_DEBUG=0`, because the debug-info-laden
test harness is ~280 MB and most of a test rebuild is that link.

## Fast optimized build: `dist-fast`

`[profile.dist-fast]` inherits `release` but drops LTO, raises codegen units to
16 and enables incremental:

```powershell
powershell -NoProfile -File scripts/gpui-fast.ps1 -Mode build
# or
cargo build --profile dist-fast --target x86_64-pc-windows-msvc -p cap-desktop-gpui
```

The first build of a profile is always a full one; subsequent one-file edits
reuse the profile's artifacts. Do **not** use this profile for release
artifacts or performance measurements — use `release` for those.

Measured on this machine (12 logical cores), after a 420-commit rebase:

| command | observed |
|---|---|
| `cargo check -p cap-desktop-gpui` (cold after rebase) | ~4m 16s |
| test harness build + run (cold) | ~7m 18s |
| `release` build (cold after rebase) | ~22m 21s |

Incremental runs after a small edit are much faster; measure your own cycle
rather than trusting these numbers.

## Portable build

```powershell
powershell -NoProfile -File scripts/gpui-fast.ps1 -Mode package
```

Builds `dist-fast` if needed, then stages `Cap.exe`, the `cap-cli`/`cap-muxer`
sidecars, the runtime DLLs and the background/music assets under
`apps/desktop-gpui/target/portable/`, and writes a zip beside the folder.
`package-gpui-portable.ps1 -Exe <path>` can package any existing binary.

## Worktrees, target directories and paths

- Prefer one checkout and `git switch` for sequential work. Cargo's cache is
  per target directory, so a fresh worktree rebuilds everything.
- If you do use extra worktrees, give them a short path and a shared
  `CARGO_TARGET_DIR`. Long paths have already broken `whisper-rs-sys`'s CMake
  step (`FTK1011` file-tracking errors); a short target path avoids it.
- The patched `gpui` dependency lives at `../../../zed-cap`. Different worktree
  paths give Cargo different package IDs for it, so GPUI can rebuild across
  worktrees even with a shared target directory.

## Optional machine-level accelerators

These are per-machine, not repo changes:

```powershell
# Faster linker (Rust ships rust-lld with the toolchain).
# Pass -Lld to the script, or set it for one command:
$env:RUSTFLAGS = "-C linker=rust-lld"

# Compile cache, mostly useful across clean builds, branch switches and rebases.
winget install Mozilla.sccache   # or: cargo install sccache
$env:RUSTC_WRAPPER = "sccache"

# Exclude build trees from Defender (elevated PowerShell).
Add-MpPreference -ExclusionPath 'C:\development\projects\cap'
Add-MpPreference -ExclusionPath 'C:\development\projects\zed-cap'
Add-MpPreference -ExclusionPath "$env:USERPROFILE\.cargo"
Add-MpPreference -ExclusionPath "$env:USERPROFILE\.rustup"
```

## Formatting

Do not run `cargo fmt --all` in this checkout. The local rustfmt version
reformats a number of unrelated, already-committed files; format only the files
you changed (`rustfmt --edition 2024 <files>` or `cargo fmt -- <paths>`).
