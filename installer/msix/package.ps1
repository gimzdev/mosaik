#Requires -Version 5.1
<#
.SYNOPSIS
  Makes Mosaik's Store package (MSIX) from the release build, ready to upload in Partner Center.

.DESCRIPTION
  Needs the release build (mosaik.ps1 build, or cargo build --release -p mosaik-desktop
  -p mosaik-watchdog -p mosaik-ipc) and makepri.exe and makeappx.exe from the Windows SDK. The
  package is not signed: the Store signs it when it is published.

  The manifest is AppxManifest.xml beside this script, filled in from identity.json (the identity
  reserved in Partner Center) and the version in Cargo.toml.

.PARAMETER Register
  Also registers the package's folder on this PC, to try the packaged app (its startup task, the
  mosaikctl command, its own data folder) before uploading. Needs Developer Mode, which you turn on
  in Settings, System, For developers.

.EXAMPLE
  .\installer\msix\package.ps1
#>
[CmdletBinding()]
param([switch]$Register)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

$Here = $PSScriptRoot
$Root = (Resolve-Path (Join-Path $Here '..\..')).Path
$Release = Join-Path $Root 'target\release'
$Work = Join-Path $Root 'target\msix'
$Layout = Join-Path $Work 'layout'

function Fail([string]$Message) {
  throw $Message
}

# A tool of the newest installed Windows SDK that has it (makepri.exe, makeappx.exe).
function Find-SdkTool([string]$Name) {
  $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
  if (-not (Test-Path $kits)) { return $null }
  Get-ChildItem $kits -Directory |
    Where-Object { $_.Name -match '^10\.\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName "x64\$Name" } |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1
}

# Runs an SDK tool; its own lines say what went wrong, if anything.
function Invoke-SdkTool([string]$Tool, [string]$What, [string[]]$Arguments) {
  & $Tool @Arguments | Out-Host
  if ($LASTEXITCODE -ne 0) { Fail "$What failed (exit code $LASTEXITCODE); the lines above say why." }
}

# The workspace version ("1.0.0") as a package version ("1.0.0.0": the Store wants the fourth
# number at 0).
function Get-PackageVersion {
  $cargo = Get-Content -Raw -Encoding UTF8 (Join-Path $Root 'Cargo.toml')
  $match = [regex]::Match($cargo, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"')
  if (-not $match.Success) { Fail 'The version was not found in Cargo.toml ([workspace.package] version).' }
  '{0}.{1}.{2}.0' -f $match.Groups[1].Value, $match.Groups[2].Value, $match.Groups[3].Value
}

$executables = 'mosaik.exe', 'mosaik-watchdog.exe', 'mosaikctl.exe'
$missing = @($executables | Where-Object { -not (Test-Path (Join-Path $Release $_)) })
if ($missing.Count -gt 0) {
  Fail ("The release build is missing ({0}). Build first: mosaik.ps1 build" -f ($missing -join ', '))
}
$makePri = Find-SdkTool 'makepri.exe'
$makeAppx = Find-SdkTool 'makeappx.exe'
if (-not $makePri -or -not $makeAppx) {
  Fail 'makepri.exe and makeappx.exe were not found. They come with the Windows SDK: winget install Microsoft.WindowsSDK.10.0.26100'
}

$identity = Get-Content -Raw -Encoding UTF8 (Join-Path $Here 'identity.json') | ConvertFrom-Json
$version = Get-PackageVersion
$values = @{
  name                 = $identity.name
  publisher            = $identity.publisher
  publisherDisplayName = $identity.publisherDisplayName
  displayName          = $identity.displayName
  version              = $version
}

# A fresh layout: the programs, the logos, the licence and the manifest.
if (Test-Path $Layout) { Remove-Item $Layout -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $Layout 'Assets') | Out-Null
foreach ($exe in $executables) { Copy-Item (Join-Path $Release $exe) $Layout }
Copy-Item (Join-Path $Here 'Assets\*.png') (Join-Path $Layout 'Assets')
Copy-Item (Join-Path $Root 'LICENSE') (Join-Path $Layout 'LICENSE.txt')
$notices = Join-Path $Root 'target\THIRD-PARTY-NOTICES.txt'
if (Test-Path $notices) { Copy-Item $notices $Layout }

$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $Here 'AppxManifest.xml')
foreach ($key in $values.Keys) {
  $escaped = [Security.SecurityElement]::Escape([string]$values[$key])
  $manifest = $manifest.Replace("{{$key}}", $escaped)
}
if ($manifest -match '\{\{\w+\}\}') { Fail "AppxManifest.xml has a value with nothing to fill it: $($Matches[0])" }
$manifestPath = Join-Path $Layout 'AppxManifest.xml'
[IO.File]::WriteAllText($manifestPath, $manifest, (New-Object Text.UTF8Encoding $false))

# The logos come in several sizes (StoreLogo.scale-200.png...) while the manifest names one
# (Assets\StoreLogo.png): resources.pri is the index Windows picks the right size from.
$priConfig = Join-Path $Work 'priconfig.xml'
Invoke-SdkTool $makePri 'makepri (configuration)' @('createconfig', '/cf', $priConfig, '/dq', 'en-US', '/pv', '10.0.0', '/o')
# One package, not a bundle: every size stays in resources.pri rather than in resource packs.
$config = New-Object Xml.XmlDocument
$config.Load($priConfig)
$packaging = $config.DocumentElement.SelectSingleNode('packaging')
if ($packaging) { [void]$config.DocumentElement.RemoveChild($packaging) }
$config.Save($priConfig)
Invoke-SdkTool $makePri 'makepri (resource index)' @('new', '/pr', $Layout, '/cf', $priConfig, '/mn', $manifestPath, '/of', (Join-Path $Layout 'resources.pri'), '/o')

$msix = Join-Path $Work ('Mosaik_{0}_x64.msix' -f $version)
Invoke-SdkTool $makeAppx 'makeappx' @('pack', '/o', '/h', 'SHA256', '/d', $Layout, '/p', $msix)

$size = (Get-Item $msix).Length / 1MB
Write-Host ''
Write-Host ('Package: {0} ({1:N1} MB), version {2}' -f $msix, $size, $version) -ForegroundColor Green
Write-Host 'Upload it in Partner Center: Apps and games, Mosaik, the submission''s Packages page.'

if ($Register) {
  try {
    Add-AppxPackage -Register (Join-Path $Layout 'AppxManifest.xml') -ForceUpdateFromAnyVersion
    Write-Host 'Registered on this PC: Mosaik is in the Start menu, as the Store would install it.' -ForegroundColor Green
    Write-Host ('To remove it: Get-AppxPackage {0} | Remove-AppxPackage' -f $identity.name)
  }
  catch {
    Fail ("The package could not be registered: {0} Turn on Developer Mode (Settings, System, For developers) and try again." -f $_.Exception.Message)
  }
}
