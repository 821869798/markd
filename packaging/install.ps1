# mkd installer for PowerShell: downloads a prebuilt binary from GitHub Releases.
# Usage:
#   irm https://raw.githubusercontent.com/821869798/markd/master/packaging/install.ps1 | iex
#   # or with a custom destination:
#   $env:DEST = "$HOME\bin"; irm https://.../install.ps1 | iex
$ErrorActionPreference = 'Stop'

$Repo = '821869798/markd'
$Dest = if ($env:DEST) { $env:DEST } else { "$HOME\.local\bin" }
$ProgressPreference = 'SilentlyContinue'

function Fail($msg) {
    Write-Host "mkd install failed: $msg" -ForegroundColor Red
    exit 1
}

# Detect OS.
$os = 'unknown'
if ($IsWindows -or $env:OS -eq 'Windows_NT') { $os = 'windows' }
elseif ($IsMacOS) { $os = 'darwin' }
elseif ($IsLinux) { $os = 'linux' }
if ($os -eq 'unknown') { Fail 'unsupported OS' }

# Detect architecture (normalize common aliases).
$archName = $env:PROCESSOR_ARCHITECTURE
if (-not $archName) { $archName = 'AMD64' }  # 64-bit Windows always sets this
$arch = 'unknown'
if ($archName -in @('AMD64', 'x64')) { $arch = 'x86_64' }
elseif ($archName -in @('ARM64', 'aarch64')) { $arch = 'aarch64' }
if ($arch -eq 'unknown') { Fail "unsupported arch: $archName" }

if ($os -eq 'darwin') {
    if ($arch -eq 'x86_64') { Fail 'no Intel macOS build; build from source with cargo install --path .' }
    $target = 'mkd-aarch64-apple-darwin'
}
elseif ($os -eq 'linux') { $target = "mkd-$arch-unknown-linux-gnu" }
else { $target = "mkd-$arch-pc-windows-msvc" }

# Resolve the latest release tag.
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$tag = ($release.tag_name -replace '^v', '')
if (-not $tag) { Fail 'could not resolve latest release' }

Write-Host "Downloading mkd v$tag for $arch-$os..."
if ($os -eq 'windows') {
    # Windows ships a flat zip: mkd.exe at the archive root.
    $url = "https://github.com/$Repo/releases/download/v$tag/$target.zip"
    $zip = New-TemporaryFile
    Invoke-WebRequest -Uri $url -OutFile $zip
    $tmp = New-Item -ItemType Directory -Path "mkd-install-$([guid]::NewGuid())" -Force
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $binary = Join-Path $tmp 'mkd.exe'
}
else {
    $url = "https://github.com/$Repo/releases/download/v$tag/$target.tar.gz"
    $tgz = New-TemporaryFile
    Invoke-WebRequest -Uri $url -OutFile $tgz
    $tmp = New-Item -ItemType Directory -Path "mkd-install-$([guid]::NewGuid())" -Force
    if (Get-Command tar -ErrorAction SilentlyContinue) {
        tar -xzf $tgz -C $tmp
        $binary = Join-Path $tmp "$target/mkd"
    }
    else {
        Fail 'tar is required to extract the archive'
    }
}

if (-not (Test-Path $binary)) { Fail 'binary not found after extraction' }

New-Item -ItemType Directory -Path $Dest -Force | Out-Null
$installed = Join-Path $Dest 'mkd'
if ($os -eq 'windows') { $installed = "$installed.exe" }
Copy-Item $binary $installed -Force
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue

Write-Host ''
Write-Host "Installed: $installed"
Write-Host "Version:   $(& $installed --version)"
Write-Host ''

# PATH hint for Windows users (persistent, user scope).
if ($os -eq 'windows') {
    $userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($userPath -notlike "*$Dest*") {
        Write-Host "NOTE: $Dest is not on your PATH. Add it:" -ForegroundColor Yellow
        Write-Host "  [Environment]::SetEnvironmentVariable('PATH', `"$Dest;`$env:PATH`", 'User')"
    }
}
elseif ((":" + $env:PATH + ":") -notlike "*:${Dest}:*") {
    Write-Host "NOTE: $Dest is not on your PATH. Add it to your profile." -ForegroundColor Yellow
}
Write-Host "Next: run 'mkd setup' to register the shell function, then open a new terminal."
