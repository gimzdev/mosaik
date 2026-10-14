#Requires -Version 5.1
<#
.SYNOPSIS
  Checks publish.ps1 against a stand-in of Microsoft's services (store-standin.mjs, run with
  Node.js): the access token, the submission left waiting deleted, a new one made from the last
  published one, the package and "What's new" put in it with nothing else changed, the upload and
  the commit, as Microsoft documents them. Without keys, the send by hand in Partner Center: what
  it opens and copies (written instead, MOSAIK_STORE_OPEN=print), and the answer it waits for.
  Nothing is sent to Microsoft, nothing is opened, and no key of the user's is read or kept.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

$Here = $PSScriptRoot
$Root = (Resolve-Path (Join-Path $Here '..\..')).Path
$node = Get-Command node -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $node) { throw 'Node.js runs the stand-in of the Store for this test; it is not installed. Run: .\mosaik.ps1 check' }

$line = Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
$version = $line.Matches[0].Groups[1].Value
$work = Join-Path ([IO.Path]::GetTempPath()) "mosaik-publish-test-$PID"
New-Item -ItemType Directory -Force -Path $work | Out-Null
$saved = @{}
$names = 'MOSAIK_STORE_API', 'MOSAIK_STORE_LOGIN', 'MOSAIK_STORE_TENANT', 'MOSAIK_STORE_CLIENT', 'MOSAIK_STORE_KEY', 'MOSAIK_STORE_OPEN', 'APPDATA'
foreach ($n in $names) { $saved[$n] = [Environment]::GetEnvironmentVariable($n, 'Process') }
$quote = { param($s) '"' + $s + '"' }
try {
  # Any file of the package's name: the stand-in looks for its name in the upload.
  $name = "Mosaik_$version.0_x64.msix"
  $package = Join-Path $work $name
  [IO.File]::WriteAllBytes($package, [byte[]](1..64))
  $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
  $listener.Start()
  $port = $listener.LocalEndpoint.Port
  $listener.Stop()
  $report = Join-Path $work 'report.json'
  $standinArgs = @((& $quote (Join-Path $Here 'store-standin.mjs')), $port, (& $quote $report), $name, (& $quote (Join-Path $Here 'whats-new.txt')))
  $standin = Start-Process -FilePath $node.Source -ArgumentList $standinArgs -PassThru -NoNewWindow
  $watch = [Diagnostics.Stopwatch]::StartNew()
  while ($true) {
    $probe = [Net.Sockets.TcpClient]::new()
    try { $probe.Connect('127.0.0.1', $port); break } catch { if ($watch.Elapsed.TotalSeconds -gt 10) { throw 'The stand-in of the Store did not start.' } ; Start-Sleep -Milliseconds 200 } finally { $probe.Dispose() }
  }

  # 1. Sent with the stand-in's keys, without asking (-Yes).
  $env:MOSAIK_STORE_API = "http://127.0.0.1:$port/api"
  $env:MOSAIK_STORE_LOGIN = "http://127.0.0.1:$port/login"
  $env:MOSAIK_STORE_TENANT = 'test-tenant'
  $env:MOSAIK_STORE_CLIENT = 'test-client'
  $env:MOSAIK_STORE_KEY = 'test-key'
  $env:APPDATA = $work
  $publish = Join-Path $Here 'publish.ps1'
  $out = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package -Yes 2>&1 | ForEach-Object { "$_" }
  $code = $LASTEXITCODE
  if (-not $standin.WaitForExit(15000)) { Stop-Process -Id $standin.Id -Force }
  if (-not (Test-Path $report)) { throw ("The stand-in wrote no report. publish.ps1 said: {0}" -f ($out -join ' / ')) }
  $seen = Get-Content -Raw -Encoding UTF8 $report | ConvertFrom-Json
  $checks = @($seen.checks.PSObject.Properties)
  $wanted = 'token_form', 'pending_deleted', 'created_after_delete', 'update_json', 'packages_are_an_array', 'old_package_removed',
  'new_package_added', 'whats_new', 'listing_kept', 'pricing_kept', 'blob_type', 'upload_is_a_zip', 'zip_has_the_package',
  'upload_after_update', 'commit_after_upload', 'status_read_until_taken'
  $failed = @($wanted | Where-Object { -not ($checks | Where-Object Name -EQ $_ | Where-Object { $_.Value -eq $true }) })
  $odd = @($checks | Where-Object { $_.Name -in 'unexpected', 'unauthorized' } | ForEach-Object { "$($_.Name): $($_.Value)" })
  if ($code -ne 0 -or $failed.Count -gt 0 -or $odd.Count -gt 0) {
    throw ("publish.ps1 against the stand-in: exit code {0}; checks not passing: {1}; {2}; it said: {3}" -f $code, ($failed -join ', '), ($odd -join ', '), ($out -join ' / '))
  }
  Write-Host ("publish.ps1 against a stand-in of the Store: {0} checks passing ({1})." -f $wanted.Count, (@($seen.seen) -join ', ')) -ForegroundColor Green

  # 2. Without keys (none in the environment, none kept in this APPDATA): by hand in Partner
  #    Center. Its page, the package shown and "What's new" copied are written instead of done.
  #    Submitted: it ends well. Not submitted, or nobody to answer: nothing sent, a failure.
  foreach ($n in 'MOSAIK_STORE_TENANT', 'MOSAIK_STORE_CLIENT', 'MOSAIK_STORE_KEY') { [Environment]::SetEnvironmentVariable($n, $null, 'Process') }
  $env:MOSAIK_STORE_OPEN = 'print'
  $page = 'https://partner.microsoft.com/dashboard/products/9PHR0L7S1QVD/overview'
  foreach ($answer in 'y', 'n', '') {
    $out = $answer | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $said = $out -join ' '
    $shown = $said.Contains("Would open: $page") -and $said.Contains("Would show: $package") -and $said.Contains("Would copy: What's new") -and $said -match 'by hand'
    $ended = if ($answer -eq 'y') { $code -eq 0 -and $said -match 'Sent' } else { $code -ne 0 -and $said -match 'Nothing was sent' }
    if (-not $shown -or -not $ended -or $said -match 'Starting a submission|did not give access|tenant ID') {
      throw ("publish.ps1 by hand, answered '{0}': exit code {1}; it said: {2}" -f $answer, $code, ($out -join ' / '))
    }
  }
  Write-Host 'publish.ps1 without keys: by hand in Partner Center (its page, the package, What''s new copied); submitted ends well, not submitted or no answer sends nothing and fails.' -ForegroundColor Green

  # 3. -Yes without keys: nobody is there to send it by hand, so nothing is opened or sent.
  $out = '' | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package -Yes 2>&1 | ForEach-Object { "$_" }
  $code = $LASTEXITCODE
  $said = $out -join ' '
  if ($code -eq 0 -or $said -notmatch 'keys' -or $said -match 'Would open') {
    throw ("publish.ps1 -Yes without keys: exit code {0}; it said: {1}" -f $code, ($out -join ' / '))
  }
  Write-Host 'publish.ps1 -Yes without keys: it says the keys are needed, and opens and sends nothing.' -ForegroundColor Green

  # 4. With keys, answered no: nothing is sent, and it ends as a failure, so mosaik1.sh --deploy
  #    --app puts nothing online either. Nothing listens where the services would be now: any
  #    request is an error.
  $env:MOSAIK_STORE_TENANT = 'test-tenant'
  $env:MOSAIK_STORE_CLIENT = 'test-client'
  $env:MOSAIK_STORE_KEY = 'test-key'
  foreach ($answer in 'n', '') {
    $out = $answer | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $said = $out -join ' '
    if ($code -eq 0 -or $said -notmatch 'Nothing was sent' -or $said -match 'Starting a submission|did not give access|refused') {
      throw ("publish.ps1 answered '{0}': exit code {1}; it said: {2}" -f $answer, $code, ($out -join ' / '))
    }
  }
  Write-Host 'publish.ps1 answered no, or nothing: it sends nothing, says so, and ends as a failure.' -ForegroundColor Green
}
finally {
  foreach ($n in $names) { [Environment]::SetEnvironmentVariable($n, $saved[$n], 'Process') }
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
