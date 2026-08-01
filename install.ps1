# Installs HyperLab's command-line tools.
#
#   irm https://raw.githubusercontent.com/JGalego/HyperLab/main/install.ps1 | iex
#
# Fetches hyperlab-mcp.exe (a stack as an MCP server) and hyperlab-graph.exe
# (a stack as a drawing) and puts them somewhere on PATH.
#
# The desktop application is not installed by this script: it is an .msi, and
# that wants installing the way Windows installs things. It is on the same
# releases page.
#
#   $env:VERSION = 'v0.1.0'   install a particular release rather than the latest
#   $env:BIN_DIR = 'C:\bin'   install somewhere other than the default

$ErrorActionPreference = 'Stop'

$Repo = 'JGalego/HyperLab'
$Tools = @('hyperlab-mcp', 'hyperlab-graph')

function Fail($message) {
    Write-Error "install.ps1: $message"
    exit 1
}

# ------------------------------------------------------------- which machine

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x64' }
    'ARM64' { 'arm64' }
    default { Fail "no build for $($env:PROCESSOR_ARCHITECTURE) yet. Build from source: cargo install --git https://github.com/$Repo hyperlab-mcp" }
}
$target = "windows-$arch"

# ------------------------------------------------------------- which release

$version = $env:VERSION
if (-not $version) {
    # The redirect from /releases/latest names the tag, so there is no JSON
    # to parse for one field.
    try {
        $response = Invoke-WebRequest -Uri "https://github.com/$Repo/releases/latest" `
            -MaximumRedirection 0 -ErrorAction SilentlyContinue -UseBasicParsing
        $location = $response.Headers.Location
    } catch {
        $location = $_.Exception.Response.Headers.Location
    }
    if (-not $location) { Fail 'could not reach GitHub to ask what the latest release is' }
    $version = ($location -split '/')[-1]
}
if (-not $version -or $version -eq 'releases') {
    Fail "there are no releases yet. Build from source: cargo install --git https://github.com/$Repo hyperlab-mcp"
}

# ------------------------------------------------------------ where it goes

$binDir = $env:BIN_DIR
if (-not $binDir) { $binDir = Join-Path $env:LOCALAPPDATA 'HyperLab\bin' }

Write-Host "HyperLab $version for $target"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

# Downloaded to a scratch directory first, so a release that is missing one
# tool does not leave the other one installed beside a gap.
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("hyperlab-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $work | Out-Null
try {
    foreach ($tool in $Tools) {
        $asset = "$tool-$target.exe"
        $url = "https://github.com/$Repo/releases/download/$version/$asset"
        Write-Host "  fetching $asset"
        try {
            Invoke-WebRequest -Uri $url -OutFile (Join-Path $work "$tool.exe") -UseBasicParsing
        } catch {
            Fail "could not download $url — is there a $target build in $version?"
        }
    }
    foreach ($tool in $Tools) {
        Move-Item -Force (Join-Path $work "$tool.exe") (Join-Path $binDir "$tool.exe")
    }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host "Installed into ${binDir}:"
foreach ($tool in $Tools) { Write-Host "  $tool.exe" }

$onPath = ($env:PATH -split ';') -contains $binDir
if (-not $onPath) {
    Write-Host ''
    Write-Host "$binDir is not on your PATH. Add it for this user:"
    Write-Host "  [Environment]::SetEnvironmentVariable('PATH', `"`$env:PATH;$binDir`", 'User')"
}
