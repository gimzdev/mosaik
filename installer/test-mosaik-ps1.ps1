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
# The build step and what it calls; the last two are not in every version of the script.
$optional = 'Stop-RunningBuild', 'Request-Quit', 'Stop-Mosaik', 'Get-WatchdogJournal'
foreach ($name in @('Under', 'Fail', 'Get-RunningBuild', 'Invoke-Build') + $optional) {
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
function Invoke-Cargo([string]$In, [string[]]$Arguments) { $script:Done.Add("cargo $($Arguments -join ' ')") }
$script:StartRefused = $false
function Start-Process {
  param([string]$FilePath, [string[]]$ArgumentList, [switch]$Wait, [switch]$PassThru)
  # As Windows refuses a program it does not trust yet (Smart App Control and a new build).
  if ($script:StartRefused) { throw 'This program is blocked by group policy.' }
  $script:Done.Add(("start {0} {1}{2}" -f $FilePath, ($ArgumentList -join ' '), $(if ($Wait) { ' (waited)' } else { '' })).Trim())
  if ($PassThru) { [pscustomobject]@{ ExitCode = 1 } }
}
# As Windows lists them: the processes of a name, none, one or several.
function Get-Process {
  [CmdletBinding()]
  param([string[]]$Name)
  $script:Running | Where-Object { $Name -contains $_.Name }
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
  $p = [pscustomobject]@{ Id = $script:NextId; Name = $Name; Path = $Path; Quits = $Quits; Dies = $Dies; CommandLine = $CommandLine; WaitedFor = 0; Stopped = $false }
  $p | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($ms) $this.WaitedFor = $ms; $this.Quits -or ($this.Stopped -and $this.Dies) }
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
Write-Host 'mosaik.ps1 build with a Mosaik running: none, one from its folder (quit, then started again), one whose mosaikctl answers on stderr (shown, still asked), one from elsewhere (left alone), two, one Windows does not start again (said, still built), one whose mosaikctl Windows refuses (its watchdog stopped first, then it, its windows brought back quietly, said), the same with its journal not readable, its watchdog (waited for), one that does not quit (said): 10 cases passing.' -ForegroundColor Green
