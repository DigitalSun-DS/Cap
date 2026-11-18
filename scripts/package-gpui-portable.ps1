<#
.SYNOPSIS
Stage a portable Windows x64 Cap folder and zip from a GPUI binary.

.DESCRIPTION
Copies the executable, the cap-cli/cap-muxer sidecars, the runtime DLLs and the
background/music assets that the app loads from disk, writes a short README,
then zips the folder. Output defaults to apps/desktop-gpui/target/portable,
which is already ignored by Git.

.EXAMPLE
  powershell -NoProfile -File scripts/package-gpui-portable.ps1 -Exe apps/desktop-gpui/target/x86_64-pc-windows-msvc/dist-fast/cap-gpui.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Exe,
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$gpui = Join-Path $repo "apps/desktop-gpui"
$target = "x86_64-pc-windows-msvc"

if (-not (Test-Path $Exe)) { throw "Binary not found: $Exe" }
if (-not $OutDir) { $OutDir = Join-Path $gpui "target/portable" }

$version = "0.0.0"
$cargo = Get-Content (Join-Path $gpui "Cargo.toml") -Raw
if ($cargo -match '(?m)^version = "([^"]+)"') { $version = $Matches[1] }

$name = "Cap-$version-portable-win64"
$stage = Join-Path $OutDir $name
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage -Force | Out-Null

function Copy-Dlls([string]$Pattern, [string]$Destination) {
    Get-ChildItem -Path $Pattern -File -ErrorAction SilentlyContinue | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $Destination $_.Name) -Force
    }
}

Copy-Item $Exe (Join-Path $stage "Cap.exe") -Force

$binaries = Join-Path $gpui "binaries"
foreach ($sidecar in @("cap-cli", "cap-muxer")) {
    $source = Join-Path $binaries "$sidecar-$target.exe"
    if (Test-Path $source) {
        Copy-Item $source (Join-Path $stage "$sidecar.exe") -Force
    } else {
        Write-Warning "Missing sidecar: $source (run scripts/build-desktop-binaries.sh $target)"
    }
}

Copy-Dlls (Join-Path $repo "target/ffmpeg/bin/*.dll") $stage
Copy-Dlls (Join-Path $repo "target/native-deps/dxc/*.dll") $stage
Copy-Dlls (Join-Path $repo "target/native-deps/onnxruntime/lib/*.dll") $stage

$licenses = Join-Path $stage "licenses/dxc"
New-Item -ItemType Directory -Path $licenses -Force | Out-Null
Get-ChildItem -Path (Join-Path $repo "target/native-deps/dxc/LICENSE-*.txt") -File -ErrorAction SilentlyContinue |
    ForEach-Object { Copy-Item $_.FullName (Join-Path $licenses $_.Name) -Force }

$assets = Join-Path $gpui "assets"
$assetDest = Join-Path $stage "assets"
New-Item -ItemType Directory -Path (Join-Path $assetDest "backgrounds") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $assetDest "music") -Force | Out-Null
foreach ($theme in @("macOS", "blue", "cities", "dark", "orange", "purple")) {
    $from = Join-Path $assets "backgrounds/$theme"
    if (Test-Path $from) {
        Copy-Item $from (Join-Path $assetDest "backgrounds/$theme") -Recurse -Force
    }
}
Get-ChildItem -Path (Join-Path $assets "music/*.mp3") -File -ErrorAction SilentlyContinue |
    ForEach-Object { Copy-Item $_.FullName (Join-Path (Join-Path $assetDest "music") $_.Name) -Force }

$notes = @"
Cap $version - portable Windows x64 build
=================================================

Run Cap.exe. Everything it needs is in this folder; no installer and no
system-wide changes. Settings and recordings stay in the normal Cap app-data
folder, so this build shares them with any installed Cap copy.

Requires the Microsoft Visual C++ runtime, as any MSVC build does. It is
present on nearly every Windows machine; if Cap.exe fails to start with a
missing VCRUNTIME140.dll, install the x64 Visual C++ Redistributable.
"@
Set-Content -Path (Join-Path $stage "PORTABLE-README.txt") -Value $notes -Encoding UTF8

$zip = Join-Path $OutDir "$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal

Write-Output "stage=$stage"
Write-Output "zip=$zip"
Write-Output ("size_mb={0:N1}" -f ((Get-Item $zip).Length / 1MB))
Write-Output "sha256=$((Get-FileHash $zip -Algorithm SHA256).Hash)"
