#Requires -Version 5.1
<#
.SYNOPSIS
  Checks mosaik.ps1's build step around a Mosaik running from the folder it builds: with none
  running the build goes ahead (job 0132: it stopped at once, "The property 'Count' cannot be
  found on this object"); one running from that folder is asked to quit, waited for, and started
  again in the tray after the build; one from anywhere else is left alone; when its mosaikctl
  cannot run (job 0133: Smart App Control refused a new build's mosaikctl.exe), it is stopped
  instead, and its watchdog brings back its windows; its watchdog is waited for; one that does not
  quit stops the build with what to do; when Windows does not start the new build, the build is
  still done and says so.

  Invoke-Build and its helpers are read from mosaik.ps1 itself; what it starts (npm, cargo, the
  processes) is stood in for, so nothing is built, quit or started, and it takes a second.
#>
[CmdletBinding()]
param(
  # The mosaik.ps1 to check: the repository's, or a one-file copy.
  [string]$Script = (Join-Path $PSScriptRoot '..\mosaik.ps1')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Resolve-Path $Script).Path, [ref]$tokens, [ref]$errors)
if (@($errors).Count -gt 0) { throw ("{0} does not parse: {1}" -f $Script, $errors[0].Message) }
$functions = @($ast.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] }, $false))
# The build step and what it calls; the last two are not in every version of the script.
foreach ($name in 'Under', 'Fail', 'Get-RunningBuild', 'Invoke-Build', 'Stop-RunningBuild', 'Request-Quit') {
  $found = @($functions | Where-Object { $_.Name -eq $name })
  if ($found.Count -eq 0) {
    if ($name -in 'Stop-RunningBuild', 'Request-Quit') { continue }
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
function Start-Process([string]$FilePath, [string[]]$ArgumentList) {
  # As Windows refuses a program it does not trust yet (Smart App Control and a new build).
  if ($script:StartRefused) { throw 'This program is blocked by group policy.' }
  $script:Done.Add("start $FilePath $($ArgumentList -join ' ')")
}
# As Windows lists them: the processes of a name, none, one or several.
function Get-Process {
  [CmdletBinding()]
  param([string[]]$Name)
  $script:Running | Where-Object { $Name -contains $_.Name }
}
# A Mosaik (or its watchdog) that ends when it quits as asked, or when it is stopped.
function New-Mosaik([string]$Path, [bool]$Quits = $true, [string]$Name = 'mosaik', [bool]$Dies = $true) {
  $p = [pscustomobject]@{ Name = $Name; Path = $Path; Quits = $Quits; Dies = $Dies; WaitedFor = 0; Stopped = $false }
  $p | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($ms) $this.WaitedFor = $ms; $this.Quits -or ($this.Stopped -and $this.Dies) }
  $p | Add-Member -MemberType ScriptMethod -Name Kill -Value { $this.Stopped = $true }
  $p
}
$ctl = Join-Path $release 'mosaikctl.exe'

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
    # A file Windows will not run, as Smart App Control refuses one.
    Set-Content -Path $ctl -Value ''
    try {
      $mine = New-Mosaik (Join-Path $release 'mosaik.exe') $false
      $script:Running = @($mine)
      Invoke-Build
    }
    finally { Remove-Item -LiteralPath $ctl -ErrorAction SilentlyContinue }
    if (-not $mine.Stopped) { return 'not stopped' }
    if (-not $script:Done.Contains($built)) { return 'not built' }
    if (-not $script:Done.Contains($restart)) { return 'not started again' }
    if (-not ($script:Done | Where-Object { $_ -like 'say *stopped*watchdog brings back*' })) { return "nothing said ($($script:Done -join ' / '))" }
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
Write-Host 'mosaik.ps1 build with a Mosaik running: none, one from its folder (quit, then started again), one from elsewhere (left alone), two, one Windows does not start again (said, still built), one whose mosaikctl Windows refuses (stopped, said), its watchdog (waited for), one that does not quit (said): 8 cases passing.' -ForegroundColor Green
