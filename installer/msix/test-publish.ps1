#Requires -Version 5.1
<#
.SYNOPSIS
  Checks publish.ps1 against a stand-in of Microsoft's services (store-standin.mjs, run with
  Node.js): the access token, the submission left waiting deleted, a new one made from the last
  published one, the package and "What's new" put in it with nothing else changed, the upload and
  the commit, as Microsoft documents them. With a key it sends without asking; the first time
  without one it asks for the key alone (the tenant and client IDs are in identity.json), keeps it
  and sends; without it, nothing is sent. Nothing is sent to Microsoft, and no key of the user's
  is read or kept.
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
$names = 'MOSAIK_STORE_API', 'MOSAIK_STORE_LOGIN', 'MOSAIK_STORE_TENANT', 'MOSAIK_STORE_CLIENT', 'MOSAIK_STORE_KEY', 'APPDATA'
foreach ($n in $names) { $saved[$n] = [Environment]::GetEnvironmentVariable($n, 'Process') }
$quote = { param($s) '"' + $s + '"' }
$api = (Get-Content -Raw -Encoding UTF8 (Join-Path $Here 'identity.json') | ConvertFrom-Json).storeApi
$publish = Join-Path $Here 'publish.ps1'
# Any file of the package's name: the stand-in looks for its name in the upload.
$name = "Mosaik_$version.0_x64.msix"
$package = Join-Path $work $name
[IO.File]::WriteAllBytes($package, [byte[]](1..64))
$wanted = 'token_form', 'pending_deleted', 'created_after_delete', 'update_json', 'packages_are_an_array', 'old_package_removed',
'new_package_added', 'whats_new', 'listing_kept', 'pricing_kept', 'blob_type', 'upload_is_a_zip', 'zip_has_the_package',
'upload_after_update', 'commit_after_upload', 'status_read_until_taken'

# Runs publish.ps1 with `$Answers` on its input against a stand-in of the Store started for it,
# which takes only the tenant and client given and the key "test-key"; what the stand-in saw and
# what publish.ps1 said.
function Send-ToStandin([string]$Case, [string]$Answers, [string]$Tenant, [string]$Client) {
  $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
  $listener.Start()
  $port = $listener.LocalEndpoint.Port
  $listener.Stop()
  $report = Join-Path $work "report-$port.json"
  $standinArgs = @((& $quote (Join-Path $Here 'store-standin.mjs')), $port, (& $quote $report), $name, (& $quote (Join-Path $Here 'whats-new.txt')), $Tenant, $Client, 'test-key')
  $standin = Start-Process -FilePath $node.Source -ArgumentList $standinArgs -PassThru -NoNewWindow
  $watch = [Diagnostics.Stopwatch]::StartNew()
  while ($true) {
    $probe = [Net.Sockets.TcpClient]::new()
    try { $probe.Connect('127.0.0.1', $port); break } catch { if ($watch.Elapsed.TotalSeconds -gt 10) { throw 'The stand-in of the Store did not start.' } ; Start-Sleep -Milliseconds 200 } finally { $probe.Dispose() }
  }
  $env:MOSAIK_STORE_API = "http://127.0.0.1:$port/api"
  $env:MOSAIK_STORE_LOGIN = "http://127.0.0.1:$port/login"
  $out = $Answers | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package 2>&1 | ForEach-Object { "$_" }
  $code = $LASTEXITCODE
  if (-not $standin.WaitForExit(15000)) { Stop-Process -Id $standin.Id -Force }
  if (-not (Test-Path $report)) { throw ("{0}: the stand-in wrote no report. publish.ps1 said: {1}" -f $Case, ($out -join ' / ')) }
  $seen = Get-Content -Raw -Encoding UTF8 $report | ConvertFrom-Json
  $checks = @($seen.checks.PSObject.Properties)
  $failed = @($wanted | Where-Object { -not ($checks | Where-Object Name -EQ $_ | Where-Object { $_.Value -eq $true }) })
  $odd = @($checks | Where-Object { $_.Name -in 'unexpected', 'unauthorized' } | ForEach-Object { "$($_.Name): $($_.Value)" })
  if ($code -ne 0 -or $failed.Count -gt 0 -or $odd.Count -gt 0) {
    throw ("{0}: exit code {1}; checks not passing: {2}; {3}; it said: {4}" -f $Case, $code, ($failed -join ', '), ($odd -join ', '), ($out -join ' / '))
  }
  [pscustomobject]@{ Said = ($out -join ' '); Seen = @($seen.seen) }
}

# A data folder of its own for each case: the key publish.ps1 keeps goes there.
function Use-Data([string]$Case) {
  $env:APPDATA = Join-Path $work $Case
  New-Item -ItemType Directory -Force -Path $env:APPDATA | Out-Null
  Join-Path $env:APPDATA 'Mosaik\store-key.xml'
}

try {
  # 1. Everything in the environment (a build server), nobody to answer: sent, and nothing asked.
  $null = Use-Data 'environment'
  $env:MOSAIK_STORE_TENANT = 'test-tenant'
  $env:MOSAIK_STORE_CLIENT = 'test-client'
  $env:MOSAIK_STORE_KEY = 'test-key'
  $sent = Send-ToStandin 'everything in the environment' '' 'test-tenant' 'test-client'
  if ($sent.Said -match 'Key|Tenant ID|Client ID') { throw ("everything in the environment: it asked something: {0}" -f $sent.Said) }
  Write-Host ("publish.ps1 against a stand-in of the Store: {0} checks passing ({1}), nothing asked." -f $wanted.Count, ($sent.Seen -join ', ')) -ForegroundColor Green
  foreach ($n in 'MOSAIK_STORE_TENANT', 'MOSAIK_STORE_CLIENT', 'MOSAIK_STORE_KEY') { [Environment]::SetEnvironmentVariable($n, $null, 'Process') }

  # 2. The first run without a key: the key alone asked once, with identity.json's tenant and
  #    client, kept for this user, and sent.
  $keyFile = Use-Data 'first-run'
  $sent = Send-ToStandin 'the first run, the key given' 'test-key' $api.tenantId $api.clientId
  if (-not (Test-Path $keyFile)) { throw "the first run, the key given: it was not kept ($keyFile)" }
  if ($sent.Said -notmatch 'Key' -or $sent.Said -match 'Tenant ID|Client ID') { throw ("the first run, the key given: not the key alone asked for: {0}" -f $sent.Said) }
  if ((Get-Content -Raw $keyFile) -match 'test-key') { throw "the first run, the key given: kept as it was typed ($keyFile)" }
  # 3. The next run: the kept key, nothing asked.
  $sent = Send-ToStandin 'the next run' '' $api.tenantId $api.clientId
  if ($sent.Said -match 'Key|Tenant ID|Client ID') { throw ("the next run: it asked something: {0}" -f $sent.Said) }
  Write-Host ("publish.ps1 the first time without a key: asks for the key alone ({0}, client {1} from identity.json), keeps it encrypted, sends; the next time it sends without asking." -f $api.tenant, $api.clientId) -ForegroundColor Green

  # 4. No key given: nothing kept, nothing sent, a failure that says what is needed, so
  #    mosaik1.sh --deploy --app puts nothing online either. Nothing listens where the services
  #    would be: any request is an error.
  $env:MOSAIK_STORE_API = 'http://127.0.0.1:9/api'
  $env:MOSAIK_STORE_LOGIN = 'http://127.0.0.1:9/login'
  $keyFile = Use-Data 'no-key'
  $out = '' | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package 2>&1 | ForEach-Object { "$_" }
  $code = $LASTEXITCODE
  $said = $out -join ' '
  if ($code -eq 0 -or $said -notmatch 'The key is needed' -or (Test-Path $keyFile) -or $said -match 'Starting a submission|did not give access') {
    throw ("publish.ps1 without a key given: exit code {0}; it said: {1}" -f $code, ($out -join ' / '))
  }
  Write-Host 'publish.ps1 without a key given: it says what is needed, keeps and sends nothing, and ends as a failure.' -ForegroundColor Green

  # 5. -Yes without a key (a script): nobody to ask, so nothing is sent.
  $null = Use-Data 'script'
  $out = '' | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package -Yes 2>&1 | ForEach-Object { "$_" }
  $code = $LASTEXITCODE
  $said = $out -join ' '
  if ($code -eq 0 -or $said -notmatch 'no key' -or $said -match 'Partner Center shows a new one') {
    throw ("publish.ps1 -Yes without a key: exit code {0}; it said: {1}" -f $code, ($out -join ' / '))
  }
  Write-Host 'publish.ps1 -Yes without a key: it says the key is needed, and asks and sends nothing.' -ForegroundColor Green
}
finally {
  foreach ($n in $names) { [Environment]::SetEnvironmentVariable($n, $saved[$n], 'Process') }
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
