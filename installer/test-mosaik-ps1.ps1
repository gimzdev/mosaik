#Requires -Version 5.1
<#
.SYNOPSIS
  Checks mosaik.ps1's build step around a Mosaik running from the folder it builds: with none
  running the build goes ahead (job 0132: it stopped at once, "The property 'Count' cannot be
  found on this object"); one running from that folder is asked to quit, waited for, and started
  again in the tray after the build; what its mosaikctl answers on stderr is shown and not taken
  for Windows refusing to run it; one from anywhere else is left alone; when its mosaikctl
  cannot run (job 0133: Smart App Control refused a new build's mosaikctl.exe), it is stopped
  instead, its watchdog first, and the windows it hid are brought back by the watchdog's quiet
  rescue, from the journal its watchdog was watching (job 0134: stopped alone, Mosaik left its
  watchdog showing "Mosaik has stopped" and the build waiting); its watchdog is waited for when it
  quits; one that does not quit stops the build with what to do; when Windows does not start the
  new build, the build is still done and says so.

  Invoke-Build and its helpers are read from mosaik.ps1 itself; what it starts (npm, cargo, the
  processes) is stood in for, so nothing is built, quit or started, and it takes a second.
#>
[CmdletBinding()]
param(
  # The mosaik.ps1 to check: a one-file copy, or by default the repository's.
  [string]$Script
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0
# Here, not as the parameter's default: there, Windows PowerShell 5.1 started with -File has no
# $PSScriptRoot yet (job 0139: the test stopped at once, in Join-Path).
if (-not $Script) { $Script = Join-Path $PSScriptRoot '..\mosaik.ps1' }

$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Resolve-Path $Script).Path, [ref]$tokens, [ref]$errors)
if (@($errors).Count -gt 0) { throw ("{0} does not parse: {1}" -f $Script, $errors[0].Message) }
$functions = @($ast.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] }, $false))
# The build step and what it calls; the optional ones are not in every version of the script.
$optional = 'Stop-RunningBuild', 'Request-Quit', 'Stop-Mosaik', 'Get-WatchdogJournal', 'Start-Again'
foreach ($name in @('Under', 'Fail', 'Get-RunningBuild', 'Invoke-Build', 'Invoke-Publish') + $optional) {
  $found = @($functions | Where-Object { $_.Name -eq $name })
  if ($found.Count -eq 0) {
    if ($name -in $optional) { continue }
    throw "$Script has no function $name."
  }
  . ([scriptblock]::Create($found[0].Extent.Text))
}

$work = Join-Path ([IO.Path]::GetTempPath()) "mosaik-build-test-$PID"
$release = [IO.Path]::Combine($work, 'target', 'release')
New-Item -ItemType Directory -Force -Path $release, ([IO.Path]::Combine($work, 'apps', 'desktop')) | Out-Null
Set-Content -Path (Join-Path $work 'THIRD-PARTY-NOTICES.txt') -Value 'notices'

# What Invoke-Build does, in order.
$script:Done = New-Object 'System.Collections.Generic.List[string]'
$script:Running = @()
function Say([string]$Text) { $script:Done.Add("say $Text") }
function Step([string]$Text) { }
function Get-Source { $work }
function Use-Cargo { }
function Invoke-Tool([string]$In, [string]$Program, [string[]]$Arguments) { $script:Done.Add("$Program $($Arguments -join ' ')") }
# A build that fails, as cargo stops at an error.
$script:CargoFails = $false
function Invoke-Cargo([string]$In, [string[]]$Arguments) {
  $script:Done.Add("cargo $($Arguments -join ' ')")
  if ($script:CargoFails) { throw 'cargo build --release failed (exit code 101).' }
}
$script:StartRefused = $false
# How many starts Windows refuses before it lets one through, as Smart App Control does with a new
# build for a minute or two; and whether the Mosaik started leaves at once (another took its place).
$script:StartRefusals = 0
$script:StartLeaves = $false
function Start-Process {
  param([string]$FilePath, [string[]]$ArgumentList, [switch]$Wait, [switch]$PassThru)
  # As Windows refuses a program it does not trust yet (Smart App Control and a new build).
  if ($script:StartRefused) { throw 'This program is blocked by group policy.' }
  if ($script:StartRefusals -gt 0) {
    $script:StartRefusals -= 1
    throw 'This program is blocked by group policy.'
  }
  $script:Done.Add(("start {0} {1}{2}" -f $FilePath, ($ArgumentList -join ' '), $(if ($Wait) { ' (waited)' } else { '' })).Trim())
  # A Mosaik started runs from then on, unless it leaves at once.
  if (-not $Wait -and -not $script:StartLeaves -and (Split-Path -Leaf $FilePath) -eq 'mosaik.exe') { $script:Running += New-Mosaik $FilePath }
  if ($PassThru) { [pscustomobject]@{ ExitCode = 1 } }
}
function Start-Sleep { param([int]$Seconds, [int]$Milliseconds) $script:Done.Add("sleep $Seconds s $Milliseconds ms") }
# As Windows lists them: the processes of a name still running, none, one or several.
function Get-Process {
  [CmdletBinding()]
  param([string[]]$Name)
  $script:Running | Where-Object { $Name -contains $_.Name -and -not $_.Ended }
}
# The command line Windows keeps for each of them (none for a process it cannot read).
function Get-CimInstance {
  [CmdletBinding()]
  param([string]$ClassName, [string]$Filter)
  $id = if ($Filter -match '(\d+)') { [int]$Matches[1] } else { -1 }
  $script:Running | Where-Object { $_.Id -eq $id -and $_.CommandLine } | ForEach-Object { [pscustomobject]@{ CommandLine = $_.CommandLine } }
}
# A Mosaik (or its watchdog) that ends when it quits as asked, or when it is stopped.
$script:NextId = 4000
function New-Mosaik([string]$Path, [bool]$Quits = $true, [string]$Name = 'mosaik', [bool]$Dies = $true, [string]$CommandLine = '') {
  $script:NextId += 1
  $p = [pscustomobject]@{ Id = $script:NextId; Name = $Name; Path = $Path; Quits = $Quits; Dies = $Dies; CommandLine = $CommandLine; WaitedFor = 0; Stopped = $false; Ended = $false }
  $p | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($ms) $this.WaitedFor = $ms; $this.Ended = $this.Quits -or ($this.Stopped -and $this.Dies); $this.Ended }
  $p | Add-Member -MemberType ScriptMethod -Name Kill -Value { $this.Stopped = $true; $script:Done.Add("stop $($this.Name)") }
  $p
}
$journal = 'C:\Users\someone\AppData\Roaming\Mosaik\hidden-windows.json'
$watchdogExe = Join-Path $release 'mosaik-watchdog.exe'
$rescue = "start $watchdogExe --rescue --quiet --journal `"$journal`" (waited)"
$ctl = Join-Path $release 'mosaikctl.exe'
# A program that runs and answers "quit" on stderr, as mosaikctl does when Mosaik does not answer:
# Windows' own tar.exe (ls where this runs on PowerShell 7 outside Windows), which takes "quit" for
# options it refuses and says so the way C programs do. (whoami.exe answered nothing that reached
# the pipe, job 0140.)
$talker = if ([IO.Path]::DirectorySeparatorChar -eq '\') { Join-Path $env:SystemRoot 'System32\tar.exe' } else { '/bin/ls' }

$built = 'cargo build --release -p mosaik-desktop -p mosaik-watchdog -p mosaik-ipc'
$restart = "start $(Join-Path $release 'mosaik.exe') --background"
$failures = @()
function Check([string]$Case, [scriptblock]$Test) {
  $script:Done.Clear()
  try {
    $problem = & $Test
    if ($problem) { $script:failures += "${Case}: $problem" }
  }
  catch { $script:failures += "${Case}: $($_.Exception.Message)" }
}

try {
  Check 'none running' {
    $script:Running = @()
    Invoke-Build
    if (-not $script:Done.Contains($built)) { return "not built ($($script:Done -join ' / '))" }
    if (@($script:Done | Where-Object { $_ -like 'start *' }).Count -gt 0) { return 'a Mosaik was started' }
  }
  Check 'one running from this folder' {
    $mine = New-Mosaik (Join-Path $release 'mosaik.exe')
    $script:Running = @($mine)
    Invoke-Build
    if ($mine.WaitedFor -le 0) { return 'not waited for' }
    $order = @($script:Done)
    $at = [array]::IndexOf($order, $built)
    if ($at -lt 0) { return "not built ($($order -join ' / '))" }
    if ([array]::IndexOf($order, $restart) -le $at) { return "not started again after the build ($($order -join ' / '))" }
    if (-not ($order | Where-Object { $_ -like 'say *quits for the build*' })) { return 'nothing said about it quitting' }
  }
  Check 'one running, its mosaikctl answering on stderr' {
    Copy-Item -LiteralPath $talker -Destination $ctl
    try {
      # Its answer, as Request-Quit will get it.
      $says = & {
        $ErrorActionPreference = 'Continue'
        @(& $ctl quit 2>&1 | ForEach-Object { "$_".Trim() } | Where-Object { $_ }) | Select-Object -First 1
      }
      $mine = New-Mosaik (Join-Path $release 'mosaik.exe')
      $script:Running = @($mine)
      Invoke-Build
    }
    finally { Remove-Item -LiteralPath $ctl -ErrorAction SilentlyContinue }
    $order = @($script:Done)
    if (-not $says) { return "$talker, standing in for mosaikctl, answered nothing" }
    if (-not ($order | Where-Object { $_.StartsWith('say ') -and $_.Contains($says) })) { return "its answer ($says) not shown ($($order -join ' / '))" }
    if ($order | Where-Object { $_ -like 'say *did not run mosaikctl*' }) { return "its answer taken for Windows refusing to run it ($($order -join ' / '))" }
    if ($mine.WaitedFor -ne 10000) { return "not waited for as one asked to quit ($($mine.WaitedFor) ms)" }
    if (-not $script:Done.Contains($restart)) { return 'not started again' }
  }
  Check 'one running from elsewhere' {
    $other = New-Mosaik 'C:\Program Files\WindowsApps\Gimzdev.Mosak_1.0.0.0_x64__yjd3d5zpwkx98\mosaik.exe'
    $script:Running = @($other)
    Invoke-Build
    if ($other.WaitedFor -ne 0) { return 'the other Mosaik was waited for' }
    if (-not $script:Done.Contains($built)) { return 'not built' }
    if ($script:Done.Contains($restart)) { return 'a Mosaik was started' }
  }
  Check 'two running, one from this folder' {
    $mine = New-Mosaik (Join-Path $release 'mosaik.exe')
    $other = New-Mosaik 'C:\Users\someone\mosaik\target\release\mosaik.exe'
    $script:Running = @($other, $mine)
    Invoke-Build
    if ($mine.WaitedFor -le 0 -or $other.WaitedFor -ne 0) { return 'the wrong one waited for' }
    if (-not $script:Done.Contains($restart)) { return 'not started again' }
  }
  Check 'one running, not started again after' {
    $script:Running = @(New-Mosaik (Join-Path $release 'mosaik.exe'))
    $script:StartRefused = $true
    try { Invoke-Build } finally { $script:StartRefused = $false }
    if (-not $script:Done.Contains($built)) { return 'not built' }
    if (-not ($script:Done | Where-Object { $_ -like 'say *built*not start it again*' })) { return "nothing said ($($script:Done -join ' / '))" }
  }
  Check 'one running, refused by Windows twice, then started' {
    $script:Running = @(New-Mosaik (Join-Path $release 'mosaik.exe'))
    $script:StartRefusals = 2
    try { Invoke-Build } finally { $script:StartRefusals = 0 }
    $order = @($script:Done)
    if (-not $script:Done.Contains($restart)) { return "not started again ($($order -join ' / '))" }
    if (@($order | Where-Object { $_ -like 'say *trying again*' }).Count -ne 2) { return "not tried again twice, saying so ($($order -join ' / '))" }
    if (-not ($order | Where-Object { $_ -like 'say *running again*' })) { return "nothing said ($($order -join ' / '))" }
  }
  Check 'one running, started again but gone at once' {
    # Another Mosaik took its place meanwhile (the Store's copy, opened from the Start menu while
    # the build ran), or it stopped: it is not running, and that is said (the owner was shown the
    # Store's 1.0.0 after building 1.0.2, with "running again" said).
    $script:Running = @(New-Mosaik (Join-Path $release 'mosaik.exe'))
    $script:StartLeaves = $true
    try { Invoke-Build } finally { $script:StartLeaves = $false }
    $order = @($script:Done)
    if ($order | Where-Object { $_ -like 'say *running again*' }) { return "it said it was running ($($order -join ' / '))" }
    if (-not ($order | Where-Object { $_ -like 'say *not running*' })) { return "nothing said ($($order -join ' / '))" }
  }
  Check 'one running, its mosaikctl refused by Windows' {
    # A file Windows will not run, as Smart App Control refuses one. Mosaik and its watchdog only
    # end when stopped; the watchdog would show its message, so it goes first.
    Set-Content -Path $ctl -Value ''
    try {
      $mine = New-Mosaik (Join-Path $release 'mosaik.exe') $false
      $dog = New-Mosaik $watchdogExe $false 'mosaik-watchdog' $true "`"$watchdogExe`" --watch $($mine.Id) --journal `"$journal`""
      $script:Running = @($mine, $dog)
      Invoke-Build
    }
    finally { Remove-Item -LiteralPath $ctl -ErrorAction SilentlyContinue }
    $order = @($script:Done)
    if (-not $mine.Stopped) { return 'not stopped' }
    $dogAt = [array]::IndexOf($order, 'stop mosaik-watchdog')
    $mineAt = [array]::IndexOf($order, 'stop mosaik')
    $rescueAt = [array]::IndexOf($order, $rescue)
    $builtAt = [array]::IndexOf($order, $built)
    if ($dogAt -lt 0 -or $dogAt -gt $mineAt) { return "its watchdog not stopped first ($($order -join ' / '))" }
    if ($rescueAt -lt $mineAt -or $rescueAt -gt $builtAt) { return "the windows not brought back before the build ($($order -join ' / '))" }
    if (-not $script:Done.Contains($restart)) { return 'not started again' }
    if (-not ($order | Where-Object { $_ -like 'say *stopped*brought back*' })) { return "nothing said ($($order -join ' / '))" }
  }
  Check 'its watchdog, its journal not readable' {
    # Windows gives no command line: the journal is the one in Mosaik's own folder.
    $mine = New-Mosaik (Join-Path $release 'mosaik.exe') $false
    $dog = New-Mosaik $watchdogExe $false 'mosaik-watchdog'
    $script:Running = @($mine, $dog)
    Invoke-Build
    $own = "start $watchdogExe --rescue --quiet --journal `"$(Join-Path $env:APPDATA 'Mosaik\hidden-windows.json')`" (waited)"
    if (-not $script:Done.Contains($own)) { return "no rescue from Mosaik's own journal ($($script:Done -join ' / '))" }
  }
  Check 'its watchdog waited for' {
    $mine = New-Mosaik (Join-Path $release 'mosaik.exe')
    $watchdog = New-Mosaik (Join-Path $release 'mosaik-watchdog.exe') $true 'mosaik-watchdog'
    $script:Running = @($mine, $watchdog)
    Invoke-Build
    if ($watchdog.WaitedFor -le 0) { return 'the watchdog was not waited for' }
    if ($watchdog.Stopped) { return 'the watchdog was stopped' }
    if (-not $script:Done.Contains($built)) { return 'not built' }
  }
  Check 'one running, the build failing' {
    # Job 0177: the build failed after Mosaik had quit for it, and none was running after. The
    # build it had been started from is still there (cargo stops before linking a new one).
    $mine = New-Mosaik (Join-Path $release 'mosaik.exe')
    $script:Running = @($mine)
    $script:CargoFails = $true
    Set-Content -Path (Join-Path $release 'mosaik.exe') -Value ''
    try { Invoke-Build } catch { $said = $_.Exception.Message } finally {
      $script:CargoFails = $false
      Remove-Item -LiteralPath (Join-Path $release 'mosaik.exe')
    }
    $order = @($script:Done)
    if (-not (Get-Variable said -ErrorAction SilentlyContinue)) { return 'the failure was not passed on' }
    if ($mine.WaitedFor -le 0) { return 'it did not quit for the build' }
    if ([array]::IndexOf($order, $restart) -le [array]::IndexOf($order, $built)) { return "the Mosaik that quit not started again ($($order -join ' / '))" }
    if (-not ($order | Where-Object { $_ -like 'say *build failed*as it was*' })) { return "nothing said ($($order -join ' / '))" }
  }
  Check 'none running, the build failing' {
    $script:Running = @()
    $script:CargoFails = $true
    try { Invoke-Build } catch { $said = $_.Exception.Message } finally { $script:CargoFails = $false }
    if (-not (Get-Variable said -ErrorAction SilentlyContinue)) { return 'the failure was not passed on' }
    if (@($script:Done | Where-Object { $_ -like 'start *' }).Count -gt 0) { return 'a Mosaik was started' }
  }
  Check 'one that does not quit' {
    $script:Running = @(New-Mosaik (Join-Path $release 'mosaik.exe') $false 'mosaik' $false)
    try { Invoke-Build } catch { $said = $_.Exception.Message }
    if (-not (Get-Variable said -ErrorAction SilentlyContinue)) { return 'the build went ahead' }
    if ($said -notmatch 'did not quit.*tray') { return "it said: $said" }
    if ($script:Done.Contains($built)) { return 'built anyway' }
  }
}
finally {
  Remove-Item -Recurse -Force -LiteralPath $work -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) { throw ("mosaik.ps1 build: {0}" -f ($failures -join '; ')) }

# mosaik.ps1 publish: the Store asked first (publish.ps1 -Check), then tests, build and package,
# then the Store. publish.ps1 is stood in for by a script ending as told. Asked first: 0 the Store
# takes this version, so the rest follows; 3 it still checks the version sent before, so nothing
# is done and publish waits (no failure, so mosaik.sh --deploy --app still puts the site online);
# 4 that one is this version, nothing done and nothing to wait for; 1 it has this version or a
# newer one, nothing done and a failure. Then the send: 0 sent, 3 waiting, anything else a failure.
# The tests run unless they passed on this same code, which is then said, and the build installs
# the interface's packages itself (the tests did it otherwise).
$work = Join-Path ([IO.Path]::GetTempPath()) "mosaik-publish-step-$PID"
$scripts = [IO.Path]::Combine($work, 'installer', 'msix')
New-Item -ItemType Directory -Force -Path $scripts | Out-Null
Set-Content -Path (Join-Path $scripts 'publish.ps1') -Value @(
  'param([switch]$KeyOnly, [switch]$Check)'
  'if ($KeyOnly) { exit 0 }'
  'if ($Check) { exit [int]$env:MOSAIK_TEST_CHECK_ENDS }'
  'exit [int]$env:MOSAIK_TEST_PUBLISH_ENDS'
)
function Invoke-Test { $script:Done.Add('test') }
function Invoke-Build { param([switch]$Installed) $script:Done.Add($(if ($Installed) { 'build (installed)' } else { 'build' })) }
function Invoke-Package { $script:Done.Add('package') }
function Get-CodeFingerprint([string]$Source) { 'this code' }
$script:TestedAt = $null
function Get-TestedAt([string]$Source, [string]$Code) { if ($Code -eq 'this code') { $script:TestedAt } else { $null } }
$publishFailures = @()
$passed = '2026-10-28 19:58'
$publishCases = @(
  @{ Check = 0; Tested = $null; Ends = 0; Waiting = $false; Fails = $false; Did = 'test,build (installed),package' },
  @{ Check = 0; Tested = $null; Ends = 3; Waiting = $true; Fails = $false; Did = 'test,build (installed),package' },
  @{ Check = 0; Tested = $null; Ends = 1; Waiting = $false; Fails = $true; Did = 'test,build (installed),package' },
  @{ Check = 0; Tested = $passed; Ends = 0; Waiting = $false; Fails = $false; Did = 'build,package' },
  @{ Check = 0; Tested = $passed; Ends = 3; Waiting = $true; Fails = $false; Did = 'build,package' },
  @{ Check = 3; Tested = $null; Ends = 0; Waiting = $true; Fails = $false; Did = '' },
  @{ Check = 4; Tested = $null; Ends = 0; Waiting = $false; Fails = $false; Did = '' },
  @{ Check = 1; Tested = $null; Ends = 0; Waiting = $false; Fails = $true; Did = '' }
)
try {
  foreach ($case in $publishCases) {
    $script:Done.Clear()
    $script:Waiting = $false
    $script:TestedAt = $case.Tested
    $env:MOSAIK_TEST_CHECK_ENDS = $case.Check
    $env:MOSAIK_TEST_PUBLISH_ENDS = $case.Ends
    $failed = $false
    try { Invoke-Publish } catch { $failed = $true }
    $did = @($script:Done | Where-Object { $_ -notlike 'say *' }) -join ','
    $said = @($script:Done | Where-Object { $_ -like 'say *' }) -join ' / '
    $skipSaid = (-not $case.Tested) -or ($said -like "*$($case.Tested)*")
    if ($failed -ne $case.Fails -or $script:Waiting -ne $case.Waiting -or $did -ne $case.Did -or -not $skipSaid) {
      $publishFailures += "publish.ps1 -Check ending with {0}, tested before: {1}, the send with {2}: failed {3}, waiting {4}, did {5}; said: {6}" -f $case.Check, [bool]$case.Tested, $case.Ends, $failed, $script:Waiting, $did, $said
    }
  }
}
finally {
  Remove-Item Env:\MOSAIK_TEST_CHECK_ENDS, Env:\MOSAIK_TEST_PUBLISH_ENDS -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force -LiteralPath $work -ErrorAction SilentlyContinue
}
if ($publishFailures.Count -gt 0) { throw ("mosaik.ps1 publish: {0}" -f ($publishFailures -join '; ')) }
Write-Host ("mosaik.ps1 publish: the Store asked first; when it takes this version, tested (unless the tests passed on this same code, said), built and packaged, then sent (0), waiting (3, not a failure) or failing (1); when it still checks the version sent before (3), nothing done and waiting; when that one is this version (4), nothing done; when it has this version or a newer one (1), nothing done and a failure: {0} cases passing." -f $publishCases.Count) -ForegroundColor Green
Write-Host 'mosaik.ps1 build with a Mosaik running: none, one from its folder (quit, then started again), one whose mosaikctl answers on stderr (shown, still asked), one from elsewhere (left alone), two, one Windows does not start again (said, still built), one Windows refuses twice (tried again, saying so, then running), one started again but gone at once (said, not called running), one whose mosaikctl Windows refuses (its watchdog stopped first, then it, its windows brought back quietly, said), the same with its journal not readable, its watchdog (waited for), a build failing with one running (it is started again, as it was, said) and with none, one that does not quit (said): 14 cases passing.' -ForegroundColor Green

# The tests once per code (Get-CodeFingerprint): the same fingerprint for the same code under the
# next version, with other notes, docs or Store listing, and with what the build and the tools
# leave in target\ and node_modules\; another for any other change (a line of code, a new file,
# a dependency's version even when it is the app's); with git, a new file it does not ignore
# counts and one it ignores does not. The record of a run that passed says when, for that code
# only.
foreach ($name in 'Get-Sha256', 'Get-TreeFiles', 'Get-CodeFingerprint', 'Get-TestedFile', 'Get-TestedAt', 'Set-Tested') {
  $found = @($functions | Where-Object { $_.Name -eq $name })
  if ($found.Count -eq 0) { throw "$Script has no function $name." }
  . ([scriptblock]::Create($found[0].Extent.Text))
}
$excludedLine = @($ast.FindAll({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$Excluded' }, $false)) | Select-Object -First 1
if (-not $excludedLine) { throw "$Script has no `$Excluded." }
. ([scriptblock]::Create($excludedLine.Extent.Text))

$work = Join-Path ([IO.Path]::GetTempPath()) "mosaik-code-test-$PID"
$codeChecks = 0
$codeFailures = New-Object 'System.Collections.Generic.List[string]'
function Put-File([string]$Relative, [string]$Text) {
  $path = [IO.Path]::Combine($work, $Relative.Replace('/', [IO.Path]::DirectorySeparatorChar))
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
  [IO.File]::WriteAllText($path, $Text)
}
# The source of a small Mosaik, the app at $App and a dependency at $Dependency in Cargo.toml,
# Cargo.lock and the interface's lock, its one line of code $Code.
function Write-Source([string]$App, [string]$Dependency, [string]$Code) {
  $text = @{
    'Cargo.toml'                     = "[workspace]`nmembers = [`"crates/mosaik-core`"]`n`n[workspace.package]`nversion = `"@APP@`"`nedition = `"2024`"`n`n[workspace.dependencies]`nitoa = `"@DEP@`"`n"
    'Cargo.lock'                     = "version = 4`n`n[[package]]`nname = `"itoa`"`nversion = `"@DEP@`"`n`n[[package]]`nname = `"mosaik-core`"`nversion = `"@APP@`"`ndependencies = [`n `"itoa`",`n]`n"
    'apps/desktop/package.json'      = "{`n  `"name`": `"mosaik-desktop`",`n  `"private`": true,`n  `"version`": `"@APP@`",`n  `"dependencies`": {`n    `"react`": `"19.2.0`"`n  }`n}`n"
    'apps/desktop/package-lock.json' = "{`n  `"name`": `"mosaik-desktop`",`n  `"version`": `"@APP@`",`n  `"lockfileVersion`": 3,`n  `"packages`": {`n    `"`": {`n      `"name`": `"mosaik-desktop`",`n      `"version`": `"@APP@`"`n    },`n    `"node_modules/react`": {`n      `"version`": `"@DEP@`"`n    }`n  }`n}`n"
    'crates/mosaik-core/src/lib.rs'  = "@CODE@`n"
    'CHANGELOG.md'                   = "# Changelog`n`n## @APP@`n`n- Notes for @APP@.`n"
    'installer/msix/whats-new.txt'   = "Mosaik @APP@: what is new.`n"
  }
  foreach ($file in $text.Keys) { Put-File $file $text[$file].Replace('@APP@', $App).Replace('@DEP@', $Dependency).Replace('@CODE@', $Code) }
}
function Check-Code([bool]$Ok, [string]$What) { $script:codeChecks++; if (-not $Ok) { $codeFailures.Add($What) } }
try {
  New-Item -ItemType Directory -Force -Path $work | Out-Null
  Write-Source '1.0.3' '1.0.3' 'pub fn one() -> u32 { 1 }'
  Put-File 'docs/STATUS.md' 'Status'
  Put-File 'installer/msix/listing/en-us/description.txt' 'A description'
  $base = Get-CodeFingerprint $work
  Check-Code ($base -match '^[0-9a-f]{64}$') "no fingerprint: $base"
  Check-Code ((Get-CodeFingerprint $work) -eq $base) 'the same code read twice gave two fingerprints'

  # The next version, its notes and "What's new": the same code
  Write-Source '1.0.4' '1.0.3' 'pub fn one() -> u32 { 1 }'
  Check-Code ((Get-CodeFingerprint $work) -eq $base) 'the next version (Cargo.toml, Cargo.lock, package.json and its lock, the changelog, What''s new) counted as other code'
  # Docs, the listing, and what the build and the tools leave behind
  Put-File 'docs/STATUS.md' 'Status, after the release'
  Put-File 'docs/adr/0042-new.md' 'A decision'
  Put-File 'docs/budgets.txt' 'Numbers'
  Put-File 'README.md' 'Mosaik'
  Put-File 'installer/msix/listing/en-us/description.txt' 'Another description'
  Put-File 'installer/msix/listing/en-us/screenshots/01-zones.png' 'picture'
  Put-File 'target/debug/deps/budget-1.exe' 'built'
  Put-File 'target/mosaik-tested.txt' 'a record'
  Put-File 'apps/desktop/node_modules/react/index.js' 'installed'
  Put-File 'apps/desktop/dist/index.html' 'built'
  Put-File 'apps/desktop/tsconfig.tsbuildinfo' 'checked'
  Check-Code ((Get-CodeFingerprint $work) -eq $base) 'docs, the Store listing, target\, node_modules\, dist\ or a .tsbuildinfo counted as code'

  # Other code: each change its own fingerprint
  $seen = @{ $base = 'the code before' }
  $changes = @(
    @{ What = 'a dependency in Cargo.lock at the app''s old version, moved'; Do = { Put-File 'Cargo.lock' ((Get-Content -Raw -LiteralPath ([IO.Path]::Combine($work, 'Cargo.lock'))).Replace("name = `"itoa`"`nversion = `"1.0.3`"", "name = `"itoa`"`nversion = `"1.0.4`"")) } },
    @{ What = 'a dependency in the interface''s lock'; Do = { Put-File 'apps/desktop/package-lock.json' ((Get-Content -Raw -LiteralPath ([IO.Path]::Combine($work, 'apps', 'desktop', 'package-lock.json'))).Replace("`"version`": `"1.0.3`"", "`"version`": `"1.0.4`"")) } },
    @{ What = 'a dependency in Cargo.toml'; Do = { Put-File 'Cargo.toml' ((Get-Content -Raw -LiteralPath ([IO.Path]::Combine($work, 'Cargo.toml'))).Replace("itoa = `"1.0.3`"", "itoa = `"1.0.4`"")) } },
    @{ What = 'a line of code'; Do = { Put-File 'crates/mosaik-core/src/lib.rs' "pub fn one() -> u32 { 2 }`n" } },
    @{ What = 'a new file'; Do = { Put-File 'crates/mosaik-core/tests/new.rs' "#[test] fn new() {}`n" } },
    @{ What = 'a script'; Do = { Put-File 'installer/msix/publish.ps1' "exit 0`n" } }
  )
  foreach ($c in $changes) {
    & $c.Do
    $now = Get-CodeFingerprint $work
    Check-Code (-not $seen.ContainsKey($now)) ("{0} gave the fingerprint of {1}" -f $c.What, $(if ($seen.ContainsKey($now)) { $seen[$now] } else { '' }))
    $seen[$now] = $c.What
  }

  # In a Git repository: the files it has and the new ones it does not ignore
  if (Get-Command git -ErrorAction SilentlyContinue) {
    Put-File '.gitignore' "/target/`nnode_modules/`ndist/`n*.log`n"
    # Git's notes on stderr (line endings) are not failures (Windows PowerShell 5.1 would stop at them)
    $stop = $ErrorActionPreference
    try {
      $ErrorActionPreference = 'Continue'
      & git init -q $work 2>&1 | Out-Null
      & git -C $work -c core.autocrlf=false add -A 2>&1 | Out-Null
    }
    finally { $ErrorActionPreference = $stop }
    $inGit = Get-CodeFingerprint $work
    Put-File 'notes.log' 'a log git ignores'
    Check-Code ((Get-CodeFingerprint $work) -eq $inGit) 'a file git ignores counted as code'
    Put-File 'crates/mosaik-core/src/more.rs' "pub fn two() -> u32 { 2 }`n"
    Check-Code ((Get-CodeFingerprint $work) -ne $inGit) 'a new file git does not ignore did not count as code'
  }

  # A run of the tests (Invoke-Test, what it runs stood in for): when every test passes, the
  # record is for the code they ran on; a run that fails leaves none, even after one that passed.
  $found = @($functions | Where-Object { $_.Name -eq 'Invoke-Test' })
  if ($found.Count -eq 0) { throw "$Script has no function Invoke-Test." }
  . ([scriptblock]::Create($found[0].Extent.Text))
  foreach ($f in 'installer/msix/test-publish.ps1', 'installer/test-mosaik-ps1.ps1', 'installer/test-mosaik-ps1-unpack.ps1') { Put-File $f "param([string]`$Script)`n" }
  $script:CargoFails = $false
  Invoke-Test
  Check-Code ($null -ne (Get-TestedAt $work (Get-CodeFingerprint $work))) 'a run of the tests that passed left no record of it'
  $script:CargoFails = $true
  $failedRun = $false
  try { Invoke-Test } catch { $failedRun = $true }
  $script:CargoFails = $false
  Check-Code $failedRun 'a run of the tests whose cargo failed did not fail'
  Check-Code ($null -eq (Get-TestedAt $work (Get-CodeFingerprint $work))) 'a run of the tests that failed left the record of the one before'

  # The record of a run that passed
  Set-Tested $work 'code-a'
  $when = Get-TestedAt $work 'code-a'
  Check-Code ("$when" -match '^\d{4}-\d\d-\d\d \d\d:\d\d$') "the record of a run that passed says when as $when"
  Check-Code ($null -eq (Get-TestedAt $work 'code-b')) 'the record of a run on other code was taken for this code'
  Set-Content -LiteralPath (Get-TestedFile $work) -Value 'not a record'
  Check-Code ($null -eq (Get-TestedAt $work 'code-a')) 'a record it cannot read was taken for a pass'
  Remove-Item -LiteralPath (Get-TestedFile $work)
  Check-Code ($null -eq (Get-TestedAt $work 'code-a')) 'no record was taken for a pass'
}
finally {
  Remove-Item -Recurse -Force -LiteralPath $work -ErrorAction SilentlyContinue
}
if ($codeFailures.Count -gt 0) { throw ("mosaik.ps1, the tests once per code: {0}" -f ($codeFailures -join '; ')) }
Write-Host ("mosaik.ps1, the tests once per code: {0} checks passing (the next version, its notes, docs, the Store listing and what builds leave are the same code; a dependency even at the app's version, a line, a new file or a script are not; with git, what it ignores is not code; a run that passes leaves the record of its code, one that fails none)." -f $codeChecks) -ForegroundColor Green
