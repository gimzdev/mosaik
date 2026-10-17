#Requires -Version 5.1
<#
.SYNOPSIS
  Checks that a one-file mosaik.ps1 builds the source inside it, even where another mosaik.ps1
  of the same version unpacked its own before (the owner, on 2026-10-16, after building from his
  mosaik.ps1: "why i got old version". It found the folder mosaik-1.0.2 that one of the day
  before had unpacked, and built that: the version had not changed, the source had).

  A small source is packed, unpacked beside its mosaik.ps1, given a build and a change of the
  user's, then changed and packed again under the same version. The new mosaik.ps1 brings the
  folder to its source: files changed, added and gone; the build kept; the user's change kept
  aside, never lost; then nothing more on the next run. A folder unpacked by a mosaik.ps1 from
  before this check (no record of its source) is brought up to date the same way, its files
  still dated as the source inside left them taken for the old source's.
#>
[CmdletBinding()]
param(
  # The mosaik.ps1 to check: by default the repository's.
  [string]$Script
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0
if (-not $Script) { $Script = Join-Path $PSScriptRoot '..\mosaik.ps1' }
$Script = (Resolve-Path $Script).Path
# The PowerShell running this, for the copies of mosaik.ps1 it runs.
$shell = (Get-Process -Id $PID).Path

$work = Join-Path ([IO.Path]::GetTempPath()) "mosaik-unpack-test-$PID"
$failures = New-Object 'System.Collections.Generic.List[string]'
function Check([bool]$Ok, [string]$What) { if (-not $Ok) { $failures.Add($What) } }
function Put([string]$Root, [string]$Relative, [string]$Text) {
  $path = [IO.Path]::Combine($Root, $Relative)
  New-Item -ItemType Directory -Force -Path (Split-Path $path) | Out-Null
  [IO.File]::WriteAllText($path, $Text)
}
function Read-Text([string]$Root, [string]$Relative) {
  $path = [IO.Path]::Combine($Root, $Relative)
  if (Test-Path -LiteralPath $path) { [IO.File]::ReadAllText($path) } else { $null }
}
# Runs a copy of mosaik.ps1 with a command, from its own folder, as a person does; its output.
function Run([string]$Copy, [string[]]$Arguments) {
  Push-Location (Split-Path $Copy)
  try { $out = & $shell -NoProfile -ExecutionPolicy Bypass -File $Copy @Arguments 2>&1 | ForEach-Object { "$_" } }
  finally { Pop-Location }
  if ($LASTEXITCODE -ne 0) { throw ("{0} {1} failed: {2}" -f $Copy, ($Arguments -join ' '), ($out -join ' / ')) }
  $out
}
# Packs the small source; the one-file copy it makes.
function Pack {
  Run (Join-Path $src 'mosaik.ps1') @('pack', $src) | Out-Null
  [IO.Path]::Combine($src, 'target', 'mosaik.ps1')
}
function Fingerprint([string]$Copy) {
  $match = [regex]::Match([IO.File]::ReadAllText($Copy), "(?m)^\`$SourceSha256 = '([0-9a-f]*)'")
  $match.Groups[1].Value
}
function Aside { @(Get-ChildItem -LiteralPath (Join-Path $unpacked '.mosaik-replaced') -Directory -ErrorAction SilentlyContinue) }
# The fingerprint of the source the unpacked folder says it has, if it says.
function Recorded {
  $path = Join-Path $unpacked '.mosaik-source.json'
  if (Test-Path -LiteralPath $path) { (Get-Content -Raw -LiteralPath $path | ConvertFrom-Json).sha256 } else { $null }
}

try {
  # A small source: what mosaik.ps1 takes for one (Cargo.toml, crates\mosaik-core), and itself.
  $src = Join-Path $work 'source'
  Put $src 'Cargo.toml' "[workspace.package]`nversion = `"9.9.9`"`n"
  Put $src 'crates/mosaik-core/src/lib.rs' 'one'
  Put $src 'apps/desktop/src/Home.tsx' 'Arrange'
  Put $src 'docs/old.md' 'the first source only'
  Copy-Item $Script (Join-Path $src 'mosaik.ps1')

  # Packed, and unpacked beside its copy, as the owner keeps mosaik.ps1 beside his repository.
  $beside = Join-Path $work 'beside'
  New-Item -ItemType Directory -Force -Path $beside | Out-Null
  $copy = Join-Path $beside 'mosaik.ps1'
  Copy-Item (Pack) $copy
  Run $copy @('unpack') | Out-Null
  $unpacked = Join-Path $beside 'mosaik-9.9.9'
  Check ((Read-Text $unpacked 'apps/desktop/src/Home.tsx') -eq 'Arrange') 'the first source was not unpacked'

  # Its build, a change of the user's and a file of the user's own.
  Put $unpacked 'target/release/build.txt' 'a build'
  Put $unpacked 'apps/desktop/node_modules/x/index.js' 'installed'
  Put $unpacked 'crates/mosaik-core/src/lib.rs' 'changed by the user'
  Put $unpacked 'notes/mine.txt' 'the user''s own'

  # The source changes, its version not.
  Put $src 'apps/desktop/src/Home.tsx' 'Move windows'
  Put $src 'crates/mosaik-core/src/lib.rs' 'two'
  Remove-Item (Join-Path $src 'docs/old.md')
  Put $src 'docs/new.md' 'the second source only'
  Remove-Item -Recurse -Force (Join-Path $src 'target')
  Copy-Item (Pack) $copy -Force
  $second = Fingerprint $copy

  # Anything that works on the source brings the folder to the source inside: here, pack.
  $said = Run $copy @('pack')
  # Four files: two changed, one added, one gone (job 0179 was told 11 for 6: a changed file was
  # counted once as gone and once as written).
  Check (($said -join ' ') -match '\(4 files brought up to date') ('not said as 4 files brought up to date: {0}' -f (@($said | Where-Object { $_ -match 'brought up to date' }) -join ' '))
  Check (($said -join ' ') -match '1 file changed since it was unpacked is kept in') ('the file kept aside not said so: {0}' -f (@($said | Where-Object { $_ -match 'kept in' }) -join ' '))
  Check ((Read-Text $unpacked 'apps/desktop/src/Home.tsx') -eq 'Move windows') 'a changed file kept the old source'
  Check ((Read-Text $unpacked 'crates/mosaik-core/src/lib.rs') -eq 'two') 'a file the user changed was not brought to the new source'
  Check ((Read-Text $unpacked 'docs/new.md') -eq 'the second source only') 'a new file is not there'
  Check ($null -eq (Read-Text $unpacked 'docs/old.md')) 'a file the new source has not is still there'
  Check ((Read-Text $unpacked 'target/release/build.txt') -eq 'a build') 'the build was not kept'
  # Newer than the build, as written now: cargo and npm build what changed by the dates of the
  # files (job 0177: dated as the source inside dates them, older than the build, they were taken
  # for built already, and the build failed).
  $built = (Get-Item -LiteralPath ([IO.Path]::Combine($unpacked, 'target', 'release', 'build.txt'))).LastWriteTimeUtc
  Check ((Get-Item -LiteralPath ([IO.Path]::Combine($unpacked, 'apps', 'desktop', 'src', 'Home.tsx'))).LastWriteTimeUtc -ge $built) 'a changed file is dated before the build'
  Check ((Read-Text $unpacked 'apps/desktop/node_modules/x/index.js') -eq 'installed') 'the installed packages were not kept'
  Check ((Read-Text $unpacked 'notes/mine.txt') -eq 'the user''s own') 'a file of the user''s own was not left where it is'
  $kept = @(Aside)
  Check ($kept.Count -eq 1) ('the user''s change was not kept aside once ({0} folders)' -f $kept.Count)
  if ($kept.Count -eq 1) {
    Check ((Read-Text $kept[0].FullName 'crates/mosaik-core/src/lib.rs') -eq 'changed by the user') 'the user''s change was lost'
    Check (@(Get-ChildItem -Recurse -File -LiteralPath $kept[0].FullName).Count -eq 1) 'more than the user''s change was kept aside'
  }
  Check ((Recorded) -eq $second) 'the folder does not say which source it has'

  # Run again: nothing changes.
  Run $copy @('pack') | Out-Null
  Check (@(Aside).Count -eq 1) 'a second run moved files aside again'

  # One file changed, said as one (job 0181: "1 files").
  Put $src 'crates/mosaik-core/src/lib.rs' 'three'
  Copy-Item (Pack) $copy -Force
  $said = Run $copy @('pack')
  Check (($said -join ' ') -match '\(1 file brought up to date') ('one file not said as one: {0}' -f (@($said | Where-Object { $_ -match 'brought up to date' }) -join ' '))

  # A folder unpacked before this check, with no record of its source: its files as the source
  # inside dated them are the old source's; the user's own and the build stay.
  Remove-Item (Join-Path $unpacked '.mosaik-source.json') -ErrorAction SilentlyContinue
  $dated = New-Object DateTime(2026, 1, 1, 0, 0, 0, [DateTimeKind]::Local)
  foreach ($f in @(Get-ChildItem -Recurse -File -LiteralPath $unpacked | Where-Object { $_.FullName -notmatch '[\\/](target|node_modules|notes|\.mosaik-replaced)[\\/]' })) {
    $f.LastWriteTime = $dated
  }
  Put $src 'apps/desktop/src/Home.tsx' 'Move windows, again'
  Remove-Item (Join-Path $src 'docs/new.md')
  Copy-Item (Pack) $copy -Force
  $said = Run $copy @('pack')
  Check (($said -join ' ') -match '\(2 files brought up to date') ('a folder with no record: not said as 2 files brought up to date: {0}' -f (@($said | Where-Object { $_ -match 'brought up to date' }) -join ' '))
  Check ((Read-Text $unpacked 'apps/desktop/src/Home.tsx') -eq 'Move windows, again') 'a folder with no record kept its old source'
  Check ($null -eq (Read-Text $unpacked 'docs/new.md')) 'a folder with no record kept a file the new source has not'
  Check (@(Aside).Count -eq 1) 'files of a folder with no record, as the source inside dated them, were moved aside'
  Check ((Read-Text $unpacked 'notes/mine.txt') -eq 'the user''s own') 'a file of the user''s own was not left where it is (no record)'
  Check ((Recorded) -eq (Fingerprint $copy)) 'the folder with no record does not say which source it has now'
}
finally {
  Remove-Item -Recurse -Force -LiteralPath $work -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) { throw ("mosaik.ps1, a new source over one unpacked before: {0}" -f ($failures -join '; ')) }
Write-Host 'mosaik.ps1, a new source over one unpacked before (same version): files changed, added and gone, dated after the build; the build and the user''s own files kept; the user''s change kept aside; nothing on a second run; a folder with no record of its source; how many files it says: 20 checks passing.' -ForegroundColor Green
