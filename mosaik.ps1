#Requires -Version 5.1
<#
.SYNOPSIS
  Mosaik in one file: its whole source, and the steps to build, test, run and package it on any
  Windows PC. (The site is the other file: mosaik.sh.)

.DESCRIPTION
  Right-click this file and choose "Run with PowerShell" for a menu. From a terminal:

    .\mosaik.ps1 unpack [folder]   put the source in a folder (default: mosaik beside this file, whatever
                                  the version; another mosaik.ps1's source there is brought to this
                                  one's, and Mosaik's Git repository there is worked in as it is)
    .\mosaik.ps1 check             see what building needs; offers to install what is missing (winget)
    .\mosaik.ps1 build             build the interface, then mosaik.exe, its watchdog and mosaikctl
    .\mosaik.ps1 test              run the tests and checks
    .\mosaik.ps1 run               start the Mosaik just built
    .\mosaik.ps1 package           make the Store package (MSIX)
    .\mosaik.ps1 publish           test, build and package it, then send it to the Microsoft Store
    .\mosaik.ps1 pack [folder]     make a new mosaik.ps1 with the source of a folder inside
    .\mosaik.ps1 verify [folder]   check that the source inside matches a folder

  In the repository this file carries no source: it works on the repository itself, and
  "pack" makes the one-file copy (target\mosaik.ps1) to take to another PC.
#>
[CmdletBinding()]
param(
  [Parameter(Position = 0)]
  [ValidateSet('menu', 'unpack', 'check', 'build', 'test', 'run', 'package', 'publish', 'pack', 'verify')]
  [string]$Command = 'menu',
  [Parameter(Position = 1)]
  [string]$Folder
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

# Written by "pack": the version and a fingerprint of the source inside this file.
$SourceVersion = ''
$SourceSha256 = ''

# Set by publish when the Store still checks the version sent before: nothing sent, exit code 3.
$Waiting = $false
# Set once this run has said it works in the repository beside this file (publish asks for the
# source three times: tests, build, package).
$RepositoryShown = $false

# Paths the source inside never includes: what building makes, the site (mosaik.sh makes it),
# the README's pictures of the site, this PC's own settings (.env.local: the Store's key), and an
# unpacked folder's record of its source and the user's changes kept aside (`Update-Source`).
$Excluded = '^(\.git|\.agent|target|site|node_modules|dist|test-results|playwright-report|\.mosaik-replaced)(/|$)|/(node_modules|dist|target|test-results|gen)/|^mosaik\.sh$|^docs/screenshots/|(^|/)\.env[^/]*\.local$|^\.mosaik-source\.json$'
# An unpacked folder's record of the source in it: its fingerprint, and each file's. (Not $Record:
# PowerShell's names know no case, and a function's $record would stand for it in what it calls.)
$RecordFile = '.mosaik-source.json'
# The date the source inside gives its files (`New-SourceZip`): a file of an unpacked folder still
# dated so is as unpacked.
$PackedDate = New-Object DateTimeOffset(2026, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
$PayloadStart = '<#--- source: a zip, in base64 ---'
$PayloadEnd = '---#>'

function Say([string]$Text, [string]$Colour = 'Gray') { Write-Host $Text -ForegroundColor $Colour }
# A path under a folder, with this system's separator.
function Under([string]$Base) { [IO.Path]::Combine([string[]](@($Base) + $args)) }
function Step([string]$Text) { Write-Host ''; Write-Host "== $Text" -ForegroundColor Cyan }
function Fail([string]$Text) { throw $Text }

# --- Where the source is -----------------------------------------------------------------------

# A folder that holds Mosaik's source.
function Test-Source([string]$Path) {
  $Path -and (Test-Path (Under $Path 'Cargo.toml')) -and (Test-Path (Under $Path 'crates' 'mosaik-core'))
}

# The source to work on: the folder given, the repository this file is in, Mosaik's repository
# beside it (as it is), or the folder this file unpacks to: unpacked first if it is not yet, and
# brought to the source inside this file when another mosaik.ps1 put its own there (the owner,
# building from his mosaik.ps1, got the source a mosaik.ps1 of the day before had unpacked: the
# version had not changed, the source had).
function Get-Source {
  if ($Folder) {
    if (Test-Source $Folder) { return (Resolve-Path $Folder).Path }
    Fail "There is no Mosaik source in $Folder."
  }
  if (Test-Source $PSScriptRoot) { return $PSScriptRoot }
  $target = Get-DefaultFolder
  if (Test-Repository $target) {
    if (-not $script:RepositoryShown) { Show-Repository $target; $script:RepositoryShown = $true }
    return (Resolve-Path $target).Path
  }
  if (-not (Test-Source $target)) { Expand-Source $target }
  elseif ((Get-RecordedSha256 $target) -ne $SourceSha256) { Update-Source $target }
  $target
}

# The folder this file unpacks to: mosaik beside it, whatever the version, so that a newer
# mosaik.ps1 brings the same folder up to date and none piles up per version (ADR 0040).
function Get-DefaultFolder {
  Under $PSScriptRoot 'mosaik'
}

# Mosaik's source in a Git repository (the owner keeps his beside his mosaik.ps1): worked in as it
# is, never written by this file, which would make its files differ from what git has (ADR 0040).
function Test-Repository([string]$Path) {
  (Test-Source $Path) -and (Test-Path (Under $Path '.git'))
}

# Says that this file works in a repository as it is, and whether its source is the one inside.
function Show-Repository([string]$Path) {
  $root = (Resolve-Path $Path).Path
  Say ("{0} is Mosaik's Git repository: this file works in it as it is, and never writes into it." -f $root) Cyan
  $difference = Compare-Source $root
  if ($difference.Different.Count -eq 0 -and $difference.Missing.Count -eq 0) {
    Say ("Its source is the one inside this file ({0} files)." -f $difference.Files.Count) Green
  }
  else {
    $sample = @($difference.Different + $difference.Missing | Select-Object -First 5) -join ', '
    Say ("Its source is not the one inside this file ({0} files differ, {1} are not there; for example {2}): the repository's is the one used." -f $difference.Different.Count, $difference.Missing.Count, $sample) Yellow
  }
}

# --- The source inside this file -----------------------------------------------------------------

function Read-Payload {
  $text = [IO.File]::ReadAllText($PSCommandPath)
  $start = $text.LastIndexOf($PayloadStart)
  $end = $text.LastIndexOf($PayloadEnd)
  if ($start -lt 0 -or $end -le $start) { return $null }
  $base64 = $text.Substring($start + $PayloadStart.Length, $end - $start - $PayloadStart.Length) -replace '\s', ''
  if (-not $base64) { return $null }
  [Convert]::FromBase64String($base64)
}

function Get-Sha256([byte[]]$Bytes) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try { -join ($sha.ComputeHash($Bytes) | ForEach-Object { $_.ToString('x2') }) } finally { $sha.Dispose() }
}

# The source inside this file, checked: the zip's bytes.
function Get-Payload {
  $zip = Read-Payload
  if (-not $zip) {
    Fail 'This copy of mosaik.ps1 carries no source (it is the one in the repository). Run it from the repository, or make a one-file copy with: .\mosaik.ps1 pack'
  }
  if ((Get-Sha256 $zip) -ne $SourceSha256) {
    Fail 'The source inside this file is damaged (its fingerprint does not match). Get mosaik.ps1 again.'
  }
  $zip
}

# The files of the source inside this file: each relative path (forward slashes) with its bytes.
function Get-PayloadFiles([byte[]]$Zip) {
  Add-Type -AssemblyName System.IO.Compression
  $files = [ordered]@{}
  $archive = New-Object IO.Compression.ZipArchive((New-Object IO.MemoryStream(, $Zip)), [IO.Compression.ZipArchiveMode]::Read)
  try {
    foreach ($entry in $archive.Entries) {
      if ($entry.FullName.EndsWith('/')) { continue }
      $buffer = New-Object IO.MemoryStream
      $stream = $entry.Open()
      try { $stream.CopyTo($buffer) } finally { $stream.Dispose() }
      $files[$entry.FullName] = $buffer.ToArray()
    }
  }
  finally { $archive.Dispose() }
  $files
}

# Writes an unpacked folder's record of its source: the fingerprint of the source inside this file,
# and each of its files'.
function Write-Record([string]$Target, $Files) {
  $sums = [ordered]@{}
  foreach ($name in $Files.Keys) { $sums[$name] = Get-Sha256 $Files[$name] }
  $json = [ordered]@{ version = $SourceVersion; sha256 = $SourceSha256; files = $sums } | ConvertTo-Json -Depth 3
  [IO.File]::WriteAllText((Under $Target $RecordFile), $json, (New-Object Text.UTF8Encoding $false))
}

# What an unpacked folder's record says: $null when it has none (unpacked by a mosaik.ps1 from
# before records, or not unpacked at all).
function Read-Record([string]$Target) {
  $path = Under $Target $RecordFile
  if (-not (Test-Path -LiteralPath $path)) { return $null }
  try { Get-Content -Raw -Encoding UTF8 -LiteralPath $path | ConvertFrom-Json } catch { $null }
}

function Get-RecordedSha256([string]$Target) {
  $record = Read-Record $Target
  if ($record -and $record.PSObject.Properties['sha256']) { [string]$record.sha256 } else { '' }
}

function Expand-Source([string]$Target) {
  $zip = Get-Payload
  if ((Test-Path $Target) -and @(Get-ChildItem -Force $Target).Count -gt 0) {
    Fail "$Target is not empty. Choose another folder: .\mosaik.ps1 unpack <folder>"
  }
  Step "Unpacking Mosaik $SourceVersion to $Target"
  Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
  $temp = [IO.Path]::Combine([IO.Path]::GetTempPath(), "mosaik-source-$PID.zip")
  [IO.File]::WriteAllBytes($temp, $zip)
  try { [IO.Compression.ZipFile]::ExtractToDirectory($temp, $Target) } finally { Remove-Item $temp -Force }
  Write-Record $Target (Get-PayloadFiles $zip)
  Say "Done: the source is in $Target" Green
}

# Brings a folder another mosaik.ps1 unpacked to the source inside this file, keeping its build
# (target, node_modules: never part of the source). A file of the old source is replaced, or
# removed when this source has it not; one the user changed is moved first to
# .mosaik-replaced\<when>\, never lost; a file of the user's own that this source has not stays
# where it is. What the old source's files were comes from the folder's record; a folder from
# before records has none, and its files still dated as the source inside dates them are taken
# for the old source's (a change made since dates a file anew).
function Update-Source([string]$Target) {
  if (Test-Path (Under $Target '.git')) {
    Fail "$Target is a Git repository: this file never writes into one. Work in it as it is, or unpack to another folder: .\mosaik.ps1 unpack <folder>"
  }
  $zip = Get-Payload
  $new = Get-PayloadFiles $zip
  $record = Read-Record $Target
  $sums = if ($record -and $record.PSObject.Properties['files']) { $record.files } else { $null }
  Step ("Bringing the source in {0} to this mosaik.ps1's (Mosaik {1}, source {2})" -f $Target, $SourceVersion, $SourceSha256.Substring(0, 12))
  $root = (Resolve-Path $Target).Path
  $aside = Under $root '.mosaik-replaced' (Get-Date -Format 'yyyyMMdd-HHmmss')
  $changed = 0
  $moved = New-Object 'System.Collections.Generic.List[string]'
  # Whether a file is the old source's, as unpacked: by its record, else by its date.
  $asUnpacked = {
    param([string]$Name, [string]$Path, [string]$Sha)
    if ($sums) {
      $known = $sums.PSObject.Properties[$Name]
      return ($null -ne $known -and [string]$known.Value -eq $Sha)
    }
    [Math]::Abs(((Get-Item -LiteralPath $Path).LastWriteTimeUtc - $PackedDate.UtcDateTime).TotalHours) -lt 15
  }
  $wasOld = {
    param([string]$Name)
    if ($sums) { return ($null -ne $sums.PSObject.Properties[$Name]) }
    $true
  }
  foreach ($name in @(Get-TreeFiles $root)) {
    if ($name -match $Excluded) { continue }
    $path = Under $root $name
    $sha = Get-Sha256 ([IO.File]::ReadAllBytes($path))
    $wanted = $new.Contains($name)
    if ($wanted -and (Get-Sha256 $new[$name]) -eq $sha) { continue }
    $ours = & $asUnpacked $name $path $sha
    if (-not $wanted -and -not $ours -and -not $sums) { continue }
    if (-not $wanted -and -not (& $wasOld $name)) { continue }
    if (-not $ours) {
      $to = Under $aside $name
      New-Item -ItemType Directory -Force -Path (Split-Path $to) | Out-Null
      Move-Item -LiteralPath $path -Destination $to
      $moved.Add($name)
    }
    elseif (-not $wanted) {
      Remove-Item -LiteralPath $path -Force
    }
    # A file the source has is counted once, as it is written below.
    if (-not $wanted) { $changed++ }
  }
  foreach ($name in $new.Keys) {
    $path = Under $root $name
    if ((Test-Path -LiteralPath $path) -and (Get-Sha256 ([IO.File]::ReadAllBytes($path))) -eq (Get-Sha256 $new[$name])) { continue }
    # Written now, so dated now: cargo and npm build what changed by the dates of the files.
    New-Item -ItemType Directory -Force -Path (Split-Path $path) | Out-Null
    [IO.File]::WriteAllBytes($path, $new[$name])
    $changed++
  }
  Write-Record $root $new
  Say ("Done: the source in {0} is this mosaik.ps1's ({1} {2} brought up to date; the build kept)." -f $Target, $changed, $(if ($changed -eq 1) { 'file' } else { 'files' })) Green
  if ($moved.Count -gt 0) {
    $what = if ($moved.Count -eq 1) { '1 file changed since it was unpacked is' } else { "$($moved.Count) files changed since they were unpacked are" }
    Say ("{0} kept in {1}: {2}" -f $what, $aside, (@($moved | Select-Object -First 5) -join ', ')) Yellow
  }
}

# The files of a source folder, relative, with forward slashes, sorted: the ones git tracks when
# it is a repository, else every file outside the excluded folders.
function Get-SourceFiles([string]$Source) {
  $files = $null
  if ((Get-Command git -ErrorAction SilentlyContinue) -and (Test-Path (Under $Source '.git'))) {
    $files = & git -C $Source -c core.quotepath=off ls-files
    if ($LASTEXITCODE -ne 0) { $files = $null }
  }
  if (-not $files) { $files = Get-TreeFiles $Source }
  @($files | Where-Object { $_ -and $_ -notmatch $Excluded } | Sort-Object -CaseSensitive)
}

# Every file under a folder, relative, with forward slashes; the excluded folders are not walked
# at all (a built copy holds hundreds of thousands of files in target and node_modules).
function Get-TreeFiles([string]$Source) {
  $root = (Resolve-Path $Source).Path.TrimEnd('\', '/')
  $folders = New-Object 'System.Collections.Generic.Stack[string]'
  $folders.Push($root)
  while ($folders.Count -gt 0) {
    foreach ($item in Get-ChildItem -LiteralPath $folders.Pop() -Force) {
      $relative = $item.FullName.Substring($root.Length + 1).Replace('\', '/')
      if (-not $item.PSIsContainer) { $relative }
      elseif ("$relative/" -notmatch $Excluded) { $folders.Push($item.FullName) }
    }
  }
}

# A zip of the source, the same bytes for the same files (fixed dates, sorted names).
function New-SourceZip([string]$Source) {
  Add-Type -AssemblyName System.IO.Compression
  $stream = New-Object IO.MemoryStream
  $archive = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
  $date = New-Object DateTimeOffset(2026, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
  $count = 0
  foreach ($file in Get-SourceFiles $Source) {
    $entry = $archive.CreateEntry($file, [IO.Compression.CompressionLevel]::Optimal)
    $entry.LastWriteTime = $date
    $bytes = [IO.File]::ReadAllBytes((Under $Source $file))
    $out = $entry.Open()
    try { $out.Write($bytes, 0, $bytes.Length) } finally { $out.Dispose() }
    $count++
  }
  $archive.Dispose()
  [pscustomobject]@{ Bytes = $stream.ToArray(); Files = $count }
}

function Get-Version([string]$Source) {
  $cargo = Get-Content -Raw -Encoding UTF8 (Under $Source 'Cargo.toml')
  $match = [regex]::Match($cargo, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
  if (-not $match.Success) { Fail 'The version was not found in Cargo.toml.' }
  $match.Groups[1].Value
}

function Invoke-Pack {
  $source = Get-Source
  $version = Get-Version $source
  Step "Packing the source of $source"
  $zip = New-SourceZip $source
  $sha = Get-Sha256 $zip.Bytes
  # This script without the source it may carry, with the new version and fingerprint.
  $text = [IO.File]::ReadAllText($PSCommandPath)
  $at = $text.LastIndexOf($PayloadStart)
  $code = $text.Substring(0, $at).TrimEnd()
  # Line ends may be CRLF (a Windows checkout): the patterns stop at the closing quote.
  $code = $code -replace "(?m)^\`$SourceVersion = '[^']*'", "`$SourceVersion = '$version'"
  $code = $code -replace "(?m)^\`$SourceSha256 = '[^']*'", "`$SourceSha256 = '$sha'"
  $base64 = [Convert]::ToBase64String($zip.Bytes, [Base64FormattingOptions]::InsertLineBreaks)
  $out = Under $source 'target' 'mosaik.ps1'
  New-Item -ItemType Directory -Force -Path (Split-Path $out) | Out-Null
  # UTF-8 with a byte order mark, so Windows PowerShell reads the file as UTF-8.
  $content = "$code`r`n`r`n$PayloadStart`r`n$base64`r`n$PayloadEnd`r`n"
  [IO.File]::WriteAllText($out, $content, (New-Object Text.UTF8Encoding $true))
  Say ('Done: {0} ({1:N1} MB, {2} files, Mosaik {3}).' -f $out, ((Get-Item $out).Length / 1MB), $zip.Files, $version) Green
  Say 'Take it, with mosaik.sh for the site, to any Windows PC: right-click, Run with PowerShell.'
}

# How a folder's source differs from the one inside this file: its source files, the ones whose
# bytes differ, and the ones inside that it has not (compared file by file, since two zip
# libraries compress the same files differently).
function Compare-Source([string]$Root) {
  $inside = @{}
  $payload = Get-PayloadFiles (Get-Payload)
  foreach ($name in $payload.Keys) { $inside[$name] = Get-Sha256 $payload[$name] }
  $files = @(Get-SourceFiles $Root)
  [pscustomobject]@{
    Files = $files
    Different = @($files | Where-Object { $inside[$_] -ne (Get-Sha256 ([IO.File]::ReadAllBytes((Under $Root $_)))) })
    Missing = @($inside.Keys | Where-Object { $files -notcontains $_ })
  }
}

# Checks that the source inside this file is exactly the files of a folder: the same names, the
# same contents.
function Invoke-Verify {
  if (-not $Folder) { Fail 'Give the folder to compare with: .\mosaik.ps1 verify <folder>' }
  $difference = Compare-Source (Resolve-Path $Folder).Path
  if ($difference.Different.Count -gt 0 -or $difference.Missing.Count -gt 0) {
    $sample = @($difference.Different + $difference.Missing | Select-Object -First 5) -join ', '
    Fail ("The source inside this file is not the source of {0} ({1} files differ, {2} are not there; for example {3}). Make it again: .\mosaik.ps1 pack" -f $Folder, $difference.Different.Count, $difference.Missing.Count, $sample)
  }
  Say ("The source inside this file is the source of {0} ({1} files)." -f $Folder, $difference.Files.Count) Green
}

# --- What building needs -------------------------------------------------------------------------

function Find-Program([string]$Name, [string[]]$Also = @()) {
  $found = Get-Command $Name -ErrorAction SilentlyContinue
  if ($found) { return $found.Source }
  $Also | Where-Object { Test-Path $_ } | Select-Object -First 1
}

function Find-Vswhere {
  Find-Program 'vswhere' @(Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe')
}

function Test-CppTools {
  $vswhere = Find-Vswhere
  if (-not $vswhere) { return $false }
  $path = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  [bool]$path
}

# The Windows SDK tools the Store package is made with.
function Test-PackagingTools {
  $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
  if (-not (Test-Path $kits)) { return $false }
  [bool](Get-ChildItem $kits -Directory | Where-Object { $_.Name -match '^10\.\d+\.\d+\.\d+$' } |
      Where-Object { (Test-Path (Join-Path $_.FullName 'x64\makeappx.exe')) -and (Test-Path (Join-Path $_.FullName 'x64\makepri.exe')) } |
      Select-Object -First 1)
}

function Test-WebView2 {
  $id = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
  foreach ($key in "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$id", "HKCU:\Software\Microsoft\EdgeUpdate\Clients\$id") {
    $pv = (Get-ItemProperty -Path $key -Name pv -ErrorAction SilentlyContinue)
    if ($pv -and $pv.pv -and $pv.pv -ne '0.0.0.0') { return $true }
  }
  $false
}

function Get-NodeVersion {
  $node = Find-Program 'node'
  if (-not $node) { return $null }
  $text = (& $node --version) -replace '^v', ''
  try { [version]$text } catch { $null }
}

# What building needs, as (name, whether it is there, what installs it, why it is needed).
function Get-Needs {
  $cargoHome = Join-Path $env:USERPROFILE '.cargo\bin'
  $node = Get-NodeVersion
  @(
    [pscustomobject]@{ Name = 'Rust (rustup)'; Ok = [bool](Find-Program 'rustup' @(Join-Path $cargoHome 'rustup.exe')); Id = 'Rustlang.Rustup'; Args = @(); Why = 'compiles Mosaik' }
    [pscustomobject]@{ Name = 'Visual Studio C++ build tools'; Ok = (Test-CppTools); Id = 'Microsoft.VisualStudio.2022.BuildTools'; Args = @('--override', '--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'); Why = 'Rust links Windows programs with them' }
    [pscustomobject]@{ Name = 'Node.js 22.13 or later'; Ok = ($node -and $node -ge [version]'22.13'); Id = 'OpenJS.NodeJS.LTS'; Args = @(); Why = 'builds the interface' }
    [pscustomobject]@{ Name = 'WebView2 runtime'; Ok = (Test-WebView2); Id = 'Microsoft.EdgeWebView2Runtime'; Args = @(); Why = 'shows the interface (Windows 11 has it)' }
    [pscustomobject]@{ Name = 'Windows SDK (makepri, makeappx)'; Ok = (Test-PackagingTools); Id = 'Microsoft.WindowsSDK.10.0.26100'; Args = @(); Why = 'only to make the Store package' }
  )
}

function Invoke-Check {
  Step 'What building Mosaik needs'
  $needs = Get-Needs
  foreach ($n in $needs) {
    if ($n.Ok) { Say ("  ok       {0}" -f $n.Name) Green } else { Say ("  missing  {0}: {1}" -f $n.Name, $n.Why) Yellow }
  }
  $missing = @($needs | Where-Object { -not $_.Ok })
  if ($missing.Count -eq 0) { Say 'Everything is there.' Green; return $true }
  $winget = Find-Program 'winget'
  if (-not $winget) {
    Say 'winget (App Installer, from the Microsoft Store) installs these; it is not on this PC.' Yellow
    return $false
  }
  # Nobody to answer (a script or CI): say what to run instead of asking.
  if ([Console]::IsInputRedirected) {
    foreach ($n in $missing) { Say ("  winget install --id {0} --exact {1}" -f $n.Id, ($n.Args -join ' ')) }
    return $false
  }
  foreach ($n in $missing) {
    $answer = Read-Host ("Install {0} with winget ({1})? [y/N]" -f $n.Name, $n.Id)
    if ($answer -notmatch '^(y|yes|o|oui)$') { continue }
    & $winget install --id $n.Id --exact --source winget --accept-package-agreements @($n.Args)
    if ($LASTEXITCODE -ne 0) { Say "  $($n.Name) was not installed (winget exit code $LASTEXITCODE)." Yellow }
  }
  Say 'Open a new PowerShell window so newly installed tools are found, then run this again.' Cyan
  $false
}

# --- Building, testing, running, packaging -------------------------------------------------------

# Runs a program in a folder; stops with its name when it fails. The program runs as a program
# (npm.cmd rather than npm.ps1, which would run inside this script's strict settings), and what it
# writes to stderr is output, not an error: Windows PowerShell 5.1 turns those lines into
# errors when the output is redirected, though only the exit code says whether it failed.
function Invoke-Tool([string]$In, [string]$Program, [string[]]$Arguments) {
  $exe = Get-Command $Program -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $exe) { Fail "$Program was not found. Run: .\mosaik.ps1 check" }
  Say ("  > {0} {1}" -f $Program, ($Arguments -join ' ')) DarkGray
  Push-Location $In
  $stop = $ErrorActionPreference
  try {
    $ErrorActionPreference = 'Continue'
    if ([Console]::IsOutputRedirected -or [Console]::IsErrorRedirected) {
      & $exe.Source @Arguments 2>&1 | ForEach-Object { "$_" }
    }
    else {
      & $exe.Source @Arguments
    }
    $code = $LASTEXITCODE
  }
  finally {
    $ErrorActionPreference = $stop
    Pop-Location
  }
  if ($code -ne 0) { Fail ("{0} {1} failed (exit code {2}); the lines above say why." -f $Program, ($Arguments -join ' '), $code) }
}

# Rust and Node.js, which every build and test needs: found, or a failure that says what to run.
function Use-Cargo {
  $cargoHome = Join-Path $env:USERPROFILE '.cargo\bin'
  if ((Test-Path $cargoHome) -and ($env:Path -notlike "*$cargoHome*")) { $env:Path = "$cargoHome;$env:Path" }
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail 'Rust is not installed. Run: .\mosaik.ps1 check' }
  if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { Fail 'Node.js is not installed. Run: .\mosaik.ps1 check' }
}

# The files of the build in $Source\target that Windows refused to load or start since $Since:
# Smart App Control judges every new unsigned file, and now and then refuses one a build has just
# made (Code Integrity events 3033 and 3077 name it). Read from the events' own fields, so the
# language of Windows and spaces in paths do not matter.
function Get-RefusedFiles([string]$Source, [datetime]$Since) {
  $target = [IO.Path]::GetFullPath((Under $Source 'target'))
  $drive = $target.Substring(0, 2)
  $under = $target.Substring(2).TrimEnd('\') + '\'
  try {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'Microsoft-Windows-CodeIntegrity/Operational'; Id = 3033, 3077; StartTime = $Since } -ErrorAction Stop)
  }
  catch { return @() }
  $files = foreach ($e in $events) {
    foreach ($p in $e.Properties) {
      # A device path: \Device\HarddiskVolume5\Users\...\target\debug\deps\some.dll
      if ($p.Value -is [string] -and $p.Value -match '^\\Device\\[^\\]+(\\.+)$') {
        $path = $Matches[1]
        if ($path.StartsWith($under, [StringComparison]::OrdinalIgnoreCase)) { $drive + $path }
      }
    }
  }
  @($files | Sort-Object -Unique)
}

# Runs cargo. When it fails because Windows refused a file the build had just made (rustc then
# "can't find crate", or a test program does not start), those files are removed and cargo runs
# once more: a file made again is judged again, and is let through (ADR 0008; job 0094).
function Invoke-Cargo([string]$In, [string[]]$Arguments) {
  $since = (Get-Date).AddSeconds(-5)
  try { Invoke-Tool $In 'cargo' $Arguments; return }
  catch {
    $refused = @(Get-RefusedFiles $In $since)
    if ($refused.Count -eq 0) { throw }
    Say ("Windows (Smart App Control) refused {0} file(s) this build made: {1}. Making them again." -f $refused.Count, (($refused | ForEach-Object { Split-Path -Leaf $_ }) -join ', ')) Yellow
    foreach ($f in $refused) {
      # The file with every companion the build made beside it (.pdb, .d, .lib, .exp).
      $stem = [IO.Path]::GetFileNameWithoutExtension($f)
      Get-ChildItem -LiteralPath (Split-Path -Parent $f) -Filter "$stem.*" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    }
    Invoke-Tool $In 'cargo' $Arguments
  }
}

# The processes called $Name (mosaik, its watchdog) started from this folder's build: Windows
# keeps their files from being built over. Like any function's, the output is unrolled (none is
# $null, one is not a list), so callers collect it with @() before reading Count (job 0132).
function Get-RunningBuild([string]$Source, [string]$Name = 'mosaik') {
  $release = [IO.Path]::GetFullPath((Under $Source 'target' 'release')) + [IO.Path]::DirectorySeparatorChar
  Get-Process -Name $Name -ErrorAction SilentlyContinue | Where-Object {
    $path = try { $_.Path } catch { $null }
    $path -and $path.StartsWith($release, [StringComparison]::OrdinalIgnoreCase)
  }
}

# Asks Mosaik to quit (every window it hid comes back) with the build's own mosaikctl, and shows
# its answer; false when Windows does not run it (Smart App Control refuses a new build's files
# for a while: job 0133 had mosaikctl.exe refused while mosaik.exe ran).
function Request-Quit([string]$Ctl) {
  if (-not (Test-Path $Ctl)) { return $false }
  # Its answer on stderr (Mosaik not answering, say) is shown, not taken for Windows refusing to
  # run it: Windows PowerShell 5.1 would stop at that line when the output is redirected.
  $stop = $ErrorActionPreference
  try {
    $ErrorActionPreference = 'Continue'
    & $Ctl quit 2>&1 | ForEach-Object { Say "  $_" DarkGray }
    $true
  }
  catch {
    Say ("Windows did not run mosaikctl ({0})." -f $_.Exception.Message) Yellow
    $false
  }
  finally { $ErrorActionPreference = $stop }
}

# The journal a watchdog watches: its "--journal" on its command line, else the one in Mosaik's
# own folder (%APPDATA%\Mosaik), where the watchdog keeps it unless a test gave Mosaik another.
function Get-WatchdogJournal($Process) {
  $line = try { (Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $($Process.Id)" -ErrorAction Stop).CommandLine } catch { $null }
  if ($line -and $line -match '--journal\s+(?:"([^"]+)"|(\S+))') {
    if ($Matches[1]) { return $Matches[1] }
    return $Matches[2]
  }
  Join-Path $env:APPDATA 'Mosaik\hidden-windows.json'
}

# Stops a Mosaik that cannot be asked to quit, or does not, and brings back what it hid without
# the watchdog's "Mosaik has stopped" message, which would hold the build until it is closed (job
# 0134): its watchdog is stopped first, then Mosaik, then the watchdog's quiet rescue restores
# every window in the journal (ADR 0006).
function Stop-Mosaik([string]$Source, $Process) {
  $dogs = @(Get-RunningBuild $Source 'mosaik-watchdog')
  $journals = @($dogs | ForEach-Object { Get-WatchdogJournal $_ } | Select-Object -Unique)
  foreach ($d in $dogs) {
    try { $d.Kill() } catch { Write-Verbose "The watchdog had already ended: $($_.Exception.Message)" }
    $null = $d.WaitForExit(5000)
  }
  try { $Process.Kill() } catch { Write-Verbose "Mosaik had already ended: $($_.Exception.Message)" }
  if (-not $Process.WaitForExit(10000)) { Fail 'Mosaik from this folder did not quit. Quit it from its tray icon, then build again.' }
  $watchdog = Under $Source 'target' 'release' 'mosaik-watchdog.exe'
  foreach ($j in $journals) {
    try {
      $rescue = Start-Process -FilePath $watchdog -ArgumentList @('--rescue', '--quiet', '--journal', "`"$j`"") -Wait -PassThru
      # 3: some windows could not be brought back (apps running as administrator).
      if ($rescue.ExitCode -eq 3) { Say 'Some windows it hid could not be brought back (apps running as administrator): run "mosaikctl rescue" as administrator.' Yellow }
    }
    catch {
      # Mosaik brings them back itself when it starts again, after the build.
      Say ("Windows did not run the watchdog's rescue ({0}): the windows Mosaik hid come back when it starts again." -f $_.Exception.Message) Yellow
    }
  }
}

# Makes way for the build: a Mosaik running from this folder is asked to quit and waited for; one
# that cannot be asked, or does not quit, is stopped and its windows brought back (Stop-Mosaik);
# then its watchdog is waited for. True when Mosaik was running, to start it again after.
function Stop-RunningBuild([string]$Source) {
  $running = @(Get-RunningBuild $Source)
  if ($running.Count -gt 0) {
    Say 'Mosaik from this folder is running: it quits for the build, and starts again after.' Yellow
    $asked = Request-Quit (Under $Source 'target' 'release' 'mosaikctl.exe')
    foreach ($p in $running) {
      if ($p.WaitForExit($(if ($asked) { 10000 } else { 1000 }))) { continue }
      Say 'It did not quit: it is stopped instead, and every window it hid is brought back.' Yellow
      Stop-Mosaik $Source $p
    }
  }
  foreach ($w in @(Get-RunningBuild $Source 'mosaik-watchdog')) {
    if (-not $w.WaitForExit(15000)) { Fail "Mosaik's watchdog from this folder is still running. Quit Mosaik from its tray icon, then build again." }
  }
  $running.Count -gt 0
}

# -Installed: the interface's packages are installed already (publish runs the tests first).
function Invoke-Build([switch]$Installed) {
  $source = Get-Source
  Use-Cargo
  $ui = Under $source 'apps' 'desktop'
  # A Mosaik of this build that is running quits first, and starts again in the tray once the new
  # build is made; should the build fail, as it was (job 0177: none was running after).
  $restart = Stop-RunningBuild $source
  try {
    Step 'Building the interface'
    if (-not $Installed) { Invoke-Tool $ui 'npm' @('ci', '--no-audit', '--no-fund') }
    Invoke-Tool $ui 'npm' @('run', 'build')
    Step 'Building Mosaik, its watchdog and mosaikctl (release)'
    Invoke-Cargo $source @('build', '--release', '-p', 'mosaik-desktop', '-p', 'mosaik-watchdog', '-p', 'mosaik-ipc')
  }
  catch {
    if ($restart -and (Test-Path (Under $source 'target' 'release' 'mosaik.exe'))) {
      Say 'The build failed: the Mosaik that quit for it starts again, as it was.' Yellow
      Start-Again $source
    }
    throw
  }
  # The licences of what Mosaik is built with, where About finds them: beside mosaik.exe.
  Copy-Item (Under $source 'THIRD-PARTY-NOTICES.txt') (Under $source 'target' 'release')
  $exe = Under $source 'target' 'release' 'mosaik.exe'
  Say ("Done: {0}" -f $exe) Green
  if ($restart) { Start-Again $source }
}

# Starts the new build again in the tray, as Mosaik was before the build. Windows can refuse a
# new program for a minute or two while Smart App Control looks it up: it is tried again every
# 20 s, for 3 minutes. "Running again" is said only once it runs from this folder, a few seconds
# on (the owner was shown the Store's 1.0.0 after building 1.0.2, with "running again" said).
function Start-Again([string]$Source) {
  $exe = Under $Source 'target' 'release' 'mosaik.exe'
  $tries = 10
  for ($try = 1; $try -le $tries; $try++) {
    try { Start-Process $exe -ArgumentList '--background' }
    catch {
      if ($try -eq $tries) {
        Say ("Mosaik is built, but Windows did not start it again ({0}). Start it with .\mosaik.ps1 run once Windows allows it." -f $_.Exception.Message) Yellow
        return
      }
      Say 'Windows has not let the new build start yet (Smart App Control looks a new program up first): trying again in 20 s.' DarkGray
      Start-Sleep -Seconds 20
      continue
    }
    # A copy that gives way to another leaves within a second; one that replaces an older copy
    # takes a few.
    Start-Sleep -Seconds 3
    for ($look = 1; $look -le 5; $look++) {
      if (@(Get-RunningBuild $Source).Count -gt 0) {
        Say 'Mosaik is running again, in the tray.' Green
        return
      }
      Start-Sleep -Seconds 1
    }
    Say 'Mosaik was started again but is not running: start it with .\mosaik.ps1 run, or from its tray icon if another Mosaik is there.' Yellow
    return
  }
}

function Invoke-Test {
  $source = Get-Source
  Use-Cargo
  $ui = Under $source 'apps' 'desktop'
  Step 'Engine tests'
  Invoke-Cargo $source @('test', '--no-fail-fast', '--all-features', '-p', 'mosaik-core', '-p', 'mosaik-ipc', '-p', 'mosaik-win', '-p', 'mosaik-overlay', '-p', 'mosaik-engine', '-p', 'mosaik-probe')
  Step 'Lints (clippy)'
  Invoke-Cargo $source @('clippy', '--workspace', '--all-targets', '--all-features', '--', '-D', 'warnings')
  Step 'Interface checks and tests'
  Invoke-Tool $ui 'npm' @('ci', '--no-audit', '--no-fund')
  foreach ($script in 'typecheck', 'lint', 'format:check', 'test') { Invoke-Tool $ui 'npm' @('run', $script) }
  Step 'Sending to the Store, against a stand-in of its services'
  & (Under $source 'installer' 'msix' 'test-publish.ps1')
  Step 'This script: building with a Mosaik running'
  & (Under $source 'installer' 'test-mosaik-ps1.ps1') -Script $PSCommandPath
  Step 'This script: a new source over one unpacked before'
  & (Under $source 'installer' 'test-mosaik-ps1-unpack.ps1') -Script $PSCommandPath
  Say 'Every test passed.' Green
}

function Invoke-Run {
  $source = Get-Source
  $exe = Under $source 'target' 'release' 'mosaik.exe'
  if (-not (Test-Path $exe)) { Fail 'Mosaik is not built yet. Run: .\mosaik.ps1 build' }
  Start-Process $exe
  Say 'Mosaik is starting: its window opens, and its icon stays in the tray.' Green
}

function Invoke-Package {
  $source = Get-Source
  & (Under $source 'installer' 'msix' 'package.ps1')
}

# Tests, builds and packages Mosaik, then sends the package to the Microsoft Store with
# installer\msix\publish.ps1. The Store's key comes first (asked once, then kept: docs/RELEASE.md),
# so the minutes of tests and build that follow need nobody. While the Store still checks the
# version sent before, nothing is sent and $script:Waiting says so: no failure (publish ends with 3).
function Invoke-Publish {
  $source = Get-Source
  $publish = Under $source 'installer' 'msix' 'publish.ps1'
  Step "The Store's key"
  & $publish -KeyOnly
  if ($LASTEXITCODE -ne 0) { Fail "Nothing was built or sent: the Store's key is needed first (the lines above say where it is)." }
  Invoke-Test
  Invoke-Build -Installed
  Invoke-Package
  Step 'Sending it to the Microsoft Store'
  & $publish
  if ($LASTEXITCODE -eq 3) { $script:Waiting = $true; return }
  if ($LASTEXITCODE -ne 0) { Fail 'The package was not sent to the Store; the lines above say why.' }
}

# --- The menu ------------------------------------------------------------------------------------

function Show-Menu {
  $where = if (Test-Source $PSScriptRoot) { $PSScriptRoot } else { Get-DefaultFolder }
  while ($true) {
    Clear-Host
    $title = if ($SourceVersion) { "Mosaik $SourceVersion" } else { 'Mosaik' }
    Say "$title, the whole app in one file" Cyan
    Say "Source: $where" DarkGray
    Say ''
    Say '  1  Unpack the source'
    Say '  2  Check what building needs'
    Say '  3  Build Mosaik'
    Say '  4  Run the tests'
    Say '  5  Start Mosaik'
    Say '  6  Make the Store package'
    Say '  7  Send it to the Microsoft Store (tests, builds and packages it first)'
    Say '  8  Make a new mosaik.ps1 from the source'
    Say '  Q  Quit'
    Say ''
    $choice = (Read-Host 'Choose').Trim().ToUpperInvariant()
    if ($choice -eq 'Q' -or $choice -eq '') { return }
    try {
      switch ($choice) {
        '1' {
          if ($SourceSha256 -and (Test-Repository $where)) { Show-Repository $where }
          elseif (-not (Test-Source $where)) { Expand-Source $where }
          elseif ($SourceSha256 -and (Get-RecordedSha256 $where) -ne $SourceSha256) { Update-Source $where }
          else { Say "The source is already in $where." Green }
        }
        '2' { [void](Invoke-Check) }
        '3' { Invoke-Build }
        '4' { Invoke-Test }
        '5' { Invoke-Run }
        '6' { Invoke-Package }
        '7' { Invoke-Publish }
        '8' { Invoke-Pack }
        default { Say 'Type a number from the list, or Q.' Yellow }
      }
    }
    catch { Say $_.Exception.Message Red }
    [void](Read-Host 'Press Enter to go back to the menu')
  }
}

try {
  switch ($Command) {
    'menu' { Show-Menu }
    'unpack' {
      $target = if ($Folder) { $Folder } else { Get-DefaultFolder }
      if (Test-Repository $target) { Show-Repository $target }
      elseif ((Test-Source $target) -and (Get-RecordedSha256 $target) -ne $SourceSha256) { Update-Source $target }
      elseif (Test-Source $target) { Say "The source is already in $target." Green }
      else { Expand-Source $target }
    }
    'check' { if (-not (Invoke-Check)) { exit 1 } }
    'build' { Invoke-Build }
    'test' { Invoke-Test }
    'run' { Invoke-Run }
    'package' { Invoke-Package }
    'publish' { Invoke-Publish; if ($script:Waiting) { exit 3 } }
    'pack' { Invoke-Pack }
    'verify' { Invoke-Verify }
  }
}
catch {
  Write-Host $_.Exception.Message -ForegroundColor Red
  exit 1
}

<#--- source: a zip, in base64 ---
---#>
