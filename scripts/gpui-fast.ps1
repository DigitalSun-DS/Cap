<#
.SYNOPSIS
Build, check or test the GPUI desktop app without paying the release profile's
LTO cost on every iteration.

.DESCRIPTION
Modes:
  check    cargo check (fastest inner loop; no codegen or link)
  test     test harness with debug info off; optional -Filter substring
  build    optimized `dist-fast` profile (no LTO, 16 codegen units)
  package  build if needed, then stage a portable Windows x64 folder + zip

The script also puts the ffmpeg / ONNX Runtime / DXC DLL directories on PATH,
which the test binary needs. See apps/desktop-gpui/BUILDING.md.

.EXAMPLE
  powershell -NoProfile -File scripts/gpui-fast.ps1 -Mode check

.EXAMPLE
  powershell -NoProfile -File scripts/gpui-fast.ps1 -Mode test -Filter editor_effect_clipboard::tests

.EXAMPLE
  powershell -NoProfile -File scripts/gpui-fast.ps1 -Mode package
#>
[CmdletBinding()]
param(
    [ValidateSet("check", "test", "build", "package")]
    [string]$Mode = "build",
    [string]$Filter = "",
    [switch]$Lld,
    [switch]$ForceBuild
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$gpui = Join-Path $repo "apps/desktop-gpui"
$target = "x86_64-pc-windows-msvc"

$toolchain = "1.95.0"
if ($env:CAP_GPUI_RUST_TOOLCHAIN) { $toolchain = $env:CAP_GPUI_RUST_TOOLCHAIN }

$env:PATH = @(
    (Join-Path $repo "target/ffmpeg/bin"),
    (Join-Path $repo "target/native-deps/onnxruntime/lib"),
    (Join-Path $repo "target/native-deps/dxc"),
    $env:PATH
) -join ";"

if ($Lld) { $env:RUSTFLAGS = "-C linker=rust-lld" }

function Invoke-Cargo {
    param([string[]]$CargoArguments)
    Write-Host "> cargo +$toolchain $($CargoArguments -join ' ')" -ForegroundColor DarkGray
    & cargo "+$toolchain" @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "cargo exited with $LASTEXITCODE" }
}

Push-Location $gpui
try {
    switch ($Mode) {
        "check" {
            Invoke-Cargo @("check", "-p", "cap-desktop-gpui")
        }
        "test" {
            $env:CARGO_PROFILE_TEST_DEBUG = "0"
            $cargoArguments = @("test", "-p", "cap-desktop-gpui", "--bin", "cap-gpui")
            if ($Filter) { $cargoArguments += $Filter }
            Invoke-Cargo $cargoArguments
        }
        "build" {
            Invoke-Cargo @("build", "--profile", "dist-fast", "--target", $target, "-p", "cap-desktop-gpui")
        }
        "package" {
            $exe = Join-Path $gpui "target/$target/dist-fast/cap-gpui.exe"
            if ($ForceBuild -or -not (Test-Path $exe)) {
                Invoke-Cargo @("build", "--profile", "dist-fast", "--target", $target, "-p", "cap-desktop-gpui")
            }
            if (-not (Test-Path $exe)) { throw "Expected build output at $exe" }
            & (Join-Path $PSScriptRoot "package-gpui-portable.ps1") -Exe $exe
        }
    }
}
finally {
    Pop-Location
}
