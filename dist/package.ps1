# Packs a built driver into the zip that a release ships, plus its SHA-256.
# Build first (cargo build --release --locked); this script does not build.
#
#   powershell -ExecutionPolicy Bypass -File .\dist\package.ps1 -Version v0.1.0
#
# The zip holds the driver folder to register with SteamVR (mousehmd\), the
# READMEs and the license notices the DLL needs. Everything except the DLL is
# taken from the committed HEAD, not from the working tree, so a local zip
# matches the one CI makes from the same commit. Output goes to dist\out\.
param(
    # The version goes into paths that are deleted recursively below, so only
    # a plain vX.Y.Z (optionally with a -prerelease part) is accepted: no
    # separators, no "..".
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^v\d+\.\d+\.\d+(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$')]
    [string]$Version
)
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$dll = Join-Path $root 'target\release\driver_mousehmd.dll'
if (-not (Test-Path $dll)) {
    throw "$dll not found. Run cargo build --release --locked first."
}

function Invoke-Git {
    $output = & git -C $root @args
    if ($LASTEXITCODE -ne 0) { throw "git $args failed" }
    $output
}

# The DLL comes from the working tree, so say so when it differs from HEAD.
$dirty = Invoke-Git status --porcelain --untracked-files=no
if ($dirty) {
    Write-Warning "The working tree has uncommitted changes. The files in the zip come from HEAD, but the DLL was built from the working tree."
}

$name = "mousehmd-$Version"
$out = Join-Path $PSScriptRoot 'out'
$stage = Join-Path $out $name
$zip = Join-Path $out "$name.zip"
$source = Join-Path $out "$name-source.zip"
$tree = Join-Path $out "$name-source"
# Leftovers of a run that stopped part way; the extraction below fails on
# files it would overwrite. Never delete anything outside dist\out.
$outFull = [System.IO.Path]::GetFullPath($out) + [System.IO.Path]::DirectorySeparatorChar
foreach ($path in $stage, $zip, "$zip.sha256", $source, $tree) {
    if (-not [System.IO.Path]::GetFullPath($path).StartsWith($outFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to touch ${path}: it is outside $out"
    }
    if (Test-Path $path) { Remove-Item $path -Recurse -Force }
}
New-Item -ItemType Directory -Force $stage | Out-Null

# The committed files, extracted through a zip of HEAD.
$files = 'driver/mousehmd', 'LICENSE', 'README.md', 'README.en.md', 'cpp/OPENVR_LICENSE.txt', 'dist/THIRD_PARTY_LICENSES.txt'
Invoke-Git archive --format=zip -o $source HEAD -- @files | Out-Null
Expand-Archive $source -DestinationPath $tree
Remove-Item $source -Force

Move-Item (Join-Path $tree 'driver\mousehmd') (Join-Path $stage 'mousehmd')
foreach ($file in 'LICENSE', 'README.md', 'README.en.md', 'cpp\OPENVR_LICENSE.txt', 'dist\THIRD_PARTY_LICENSES.txt') {
    Move-Item (Join-Path $tree $file) $stage
}
Remove-Item $tree -Recurse -Force
$bin = Join-Path $stage 'mousehmd\bin\win64'
New-Item -ItemType Directory -Force $bin | Out-Null
Copy-Item $dll (Join-Path $bin 'driver_mousehmd.dll')

# The notices name the versions linked into the DLL. Refuse to ship them if
# Cargo.lock or the toolchain has moved on without them.
$notices = Get-Content (Join-Path $stage 'THIRD_PARTY_LICENSES.txt') -Raw
$lock = Get-Content (Join-Path $root 'Cargo.lock') -Raw
$expected = @()
foreach ($crate in 'windows-sys', 'windows-targets', 'windows_x86_64_msvc') {
    $match = [regex]::Match($lock, "(?m)^name = `"$([regex]::Escape($crate))`"\r?\nversion = `"([^`"]+)`"")
    if (-not $match.Success) { throw "$crate not found in Cargo.lock" }
    $expected += "$crate $($match.Groups[1].Value)"
}
$rustc = & rustc --version
if ($LASTEXITCODE -ne 0 -or $rustc -notmatch '^rustc (\d+\.\d+\.\d+)') { throw "could not read the rustc version" }
$rust = $Matches[1]
$expected += "Rust $rust", "/blob/$rust/COPYRIGHT"
$missing = $expected | Where-Object { -not $notices.Contains($_) }
if ($missing) {
    throw "dist\THIRD_PARTY_LICENSES.txt is out of date; it does not mention: $($missing -join ', ')"
}

# Written by hand rather than with Compress-Archive: Windows PowerShell 5.1's
# Compress-Archive stores '\' in entry names, which tools other than
# Explorer do not read as folders.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$stream = [System.IO.File]::Open($zip, [System.IO.FileMode]::CreateNew)
try {
    $archive = New-Object System.IO.Compression.ZipArchive($stream, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        Get-ChildItem $stage -Recurse -File | Sort-Object FullName | ForEach-Object {
            $entry = $_.FullName.Substring($stage.Length + 1).Replace('\', '/')
            [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $archive, $_.FullName, $entry, [System.IO.Compression.CompressionLevel]::Optimal)
        }
    } finally {
        $archive.Dispose()
    }
} finally {
    $stream.Dispose()
}
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -Path "$zip.sha256" -Value "$hash  $name.zip" -NoNewline -Encoding ascii
Remove-Item $stage -Recurse -Force

Write-Host "OK: $zip"
Write-Host "SHA-256: $hash"
