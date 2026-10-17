#Requires -Version 5.1
<#
.SYNOPSIS
  Checks publish.ps1 against a stand-in of Microsoft's services (store-standin.mjs, run with
  Node.js): the access token, the submission left waiting deleted (one still with the Store is
  left alone: nothing is sent, and the end says so apart from a failure), a new one made from the last
  published one, the package and "What's new" put in it with nothing else changed, the upload and
  the commit, as Microsoft documents them. With a key (in the environment, or in .env.local beside
  Cargo.toml, as in a web project) it sends without asking; the first time
  without one it asks for the key alone (the tenant and client IDs are in identity.json), keeps it
  and sends; without it, nothing is sent; -KeyOnly (mosaik.ps1 publish runs it first) asks for the
  key and sends nothing. At the hidden prompt, a key pasted is taken without the spaces and line
  breaks around it, and when nothing comes through (a paste in Git Bash's window), the key copied
  to the clipboard, asked up to three times. Nothing is sent to Microsoft, and no key of the user's is
  read or kept.
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
# publish.ps1 runs from a copy, with Cargo.toml and no .env.local beside it: the repository's own
# .env.local, the owner's key, is never read nor touched.
$tree = Join-Path $work 'tree'
$treeScripts = Join-Path $tree 'installer\msix'
New-Item -ItemType Directory -Force -Path $treeScripts | Out-Null
foreach ($f in 'publish.ps1', 'identity.json', 'whats-new.txt') { Copy-Item (Join-Path $Here $f) $treeScripts }
Copy-Item (Join-Path $Root 'Cargo.toml') $tree
$publish = Join-Path $treeScripts 'publish.ps1'
# Any file of the package's name: the stand-in looks for its name in the upload.
$name = "Mosaik_$version.0_x64.msix"
$package = Join-Path $work $name
[IO.File]::WriteAllBytes($package, [byte[]](1..64))
$wanted = 'token_form', 'pending_status_read', 'pending_deleted', 'created_after_delete', 'update_json', 'packages_are_an_array', 'old_package_removed',
'new_package_added', 'whats_new', 'listing_kept', 'pricing_kept', 'blob_type', 'upload_is_a_zip', 'zip_has_the_package',
'upload_after_update', 'commit_after_upload', 'status_read_until_taken'

# Runs publish.ps1 with `$Answers` on its input against a stand-in of the Store started for it,
# which takes only the tenant and client given and the key "test-key", the submission made before
# standing at `$Status`; publish.ps1's exit code and lines, and what the stand-in saw and checked.
function Invoke-Standin([string]$Case, [string]$Answers, [string]$Tenant, [string]$Client, [string]$Status = 'PendingCommit') {
  $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
  $listener.Start()
  $port = $listener.LocalEndpoint.Port
  $listener.Stop()
  $report = Join-Path $work "report-$port.json"
  $standinArgs = @((& $quote (Join-Path $Here 'store-standin.mjs')), $port, (& $quote $report), $name, (& $quote (Join-Path $Here 'whats-new.txt')), $Tenant, $Client, 'test-key', $Status)
  $standin = Start-Process -FilePath $node.Source -ArgumentList $standinArgs -PassThru -NoNewWindow
  $watch = [Diagnostics.Stopwatch]::StartNew()
  while ($true) {
    $probe = [Net.Sockets.TcpClient]::new()
    try { $probe.Connect('127.0.0.1', $port); break } catch { if ($watch.Elapsed.TotalSeconds -gt 10) { throw 'The stand-in of the Store did not start.' } ; Start-Sleep -Milliseconds 200 } finally { $probe.Dispose() }
  }
  $env:MOSAIK_STORE_API = "http://127.0.0.1:$port/api"
  $env:MOSAIK_STORE_LOGIN = "http://127.0.0.1:$port/login"
  $out = @($Answers | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -Package $package 2>&1 | ForEach-Object { "$_" })
  $code = $LASTEXITCODE
  if (-not $standin.WaitForExit(15000)) { Stop-Process -Id $standin.Id -Force }
  if (-not (Test-Path $report)) { throw ("{0}: the stand-in wrote no report. publish.ps1 said: {1}" -f $Case, ($out -join ' / ')) }
  $seen = Get-Content -Raw -Encoding UTF8 $report | ConvertFrom-Json
  [pscustomobject]@{ Code = $code; Said = ($out -join ' '); Lines = $out; Seen = @($seen.seen); Checks = @($seen.checks.PSObject.Properties) }
}

# Sends to the stand-in, which must see everything Microsoft documents and nothing else.
function Send-ToStandin([string]$Case, [string]$Answers, [string]$Tenant, [string]$Client) {
  $run = Invoke-Standin $Case $Answers $Tenant $Client
  $failed = @($wanted | Where-Object { -not ($run.Checks | Where-Object Name -EQ $_ | Where-Object { $_.Value -eq $true }) })
  $odd = @($run.Checks | Where-Object { $_.Name -in 'unexpected', 'unauthorized' } | ForEach-Object { "$($_.Name): $($_.Value)" })
  if ($run.Code -ne 0 -or $failed.Count -gt 0 -or $odd.Count -gt 0) {
    throw ("{0}: exit code {1}; checks not passing: {2}; {3}; it said: {4}" -f $Case, $run.Code, ($failed -join ', '), ($odd -join ', '), ($run.Lines -join ' / '))
  }
  $run
}

# Whether publish.ps1 asked for the key: a script answering on its input is asked with "Key" alone
# on a line.
function Test-Asked([string[]]$Lines) { @($Lines | ForEach-Object { $_.Trim() }) -contains 'Key' }

# A data folder of its own for each case: the key publish.ps1 keeps goes there.
function Use-Data([string]$Case) {
  $env:APPDATA = Join-Path $work $Case
  New-Item -ItemType Directory -Force -Path $env:APPDATA | Out-Null
  Join-Path $env:APPDATA 'Mosaik\store-key.xml'
}

try {
  # 0. The hidden prompt, as a person meets it (read from publish.ps1, Read-Host and the clipboard
  #    stood in for): what is pasted, without the spaces and line breaks around it; when nothing
  #    comes through, the key copied to the clipboard; anything else there is not taken, and the
  #    key is asked again, three times at most.
  $tokens = $null
  $errors = $null
  $ast = [Management.Automation.Language.Parser]::ParseFile($publish, [ref]$tokens, [ref]$errors)
  $functions = @($ast.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] }, $false))
  foreach ($fn in 'ConvertFrom-Secure', 'Read-Key') {
    $found = @($functions | Where-Object { $_.Name -eq $fn })
    if ($found.Count -eq 0) { throw "publish.ps1 has no function $fn." }
    . ([scriptblock]::Create($found[0].Extent.Text))
  }
  $script:Typed = New-Object System.Collections.Queue
  $script:Clipboard = ''
  $script:Said = New-Object 'System.Collections.Generic.List[string]'
  function Say([string]$Text) { $script:Said.Add($Text) }
  function Read-Host {
    param([string]$Prompt, [switch]$AsSecureString)
    $text = if ($script:Typed.Count -gt 0) { [string]$script:Typed.Dequeue() } else { '' }
    $secure = New-Object Security.SecureString
    foreach ($c in $text.ToCharArray()) { $secure.AppendChar($c) }
    $secure
  }
  function Get-Clipboard { param([switch]$Raw) $script:Clipboard }
  $prompts = @(
    @{ Case = 'pasted, with spaces and a line break'; Typed = @("  aB3~kEy.pasted-1234567`r`n"); Clipboard = 'other-key-on-the-clipboard'; Key = 'aB3~kEy.pasted-1234567' },
    @{ Case = 'nothing through, the key copied'; Typed = @(''); Clipboard = "`r`n  cOp1ed~key_from.clipboard99  `r`n"; Key = 'cOp1ed~key_from.clipboard99' },
    @{ Case = 'nothing through, other text copied, then pasted'; Typed = @('', 'thE2nd~try.pasted-key0001'); Clipboard = 'a sentence, not a key'; Key = 'thE2nd~try.pasted-key0001' },
    @{ Case = 'nothing at all'; Typed = @('', '', ''); Clipboard = ''; Key = $null }
  )
  foreach ($p in $prompts) {
    $script:Typed.Clear()
    foreach ($t in $p.Typed) { $script:Typed.Enqueue($t) }
    $script:Clipboard = $p.Clipboard
    $script:Said.Clear()
    $got = ConvertFrom-Secure (Read-Key -Console $true)
    $want = if ($null -eq $p.Key) { '' } else { $p.Key }
    if ($got -ne $want) { throw ("the hidden prompt, {0}: got {1} characters, not the key ({2})" -f $p.Case, $got.Length, ($script:Said -join ' / ')) }
    if ($script:Typed.Count -ne 0) { throw ("the hidden prompt, {0}: asked {1} time(s) too few" -f $p.Case, $script:Typed.Count) }
  }
  if (-not ($script:Said | Where-Object { $_ -match 'copy the key, then press Enter' })) { throw ("the hidden prompt: nothing said about what to do ({0})" -f ($script:Said -join ' / ')) }
  Remove-Item Function:\Read-Host, Function:\Get-Clipboard, Function:\Say
  Write-Host ("publish.ps1's hidden prompt: {0} cases passing (a key pasted with spaces and a line break around it, the key copied when nothing came through, other text copied, nothing at all)." -f $prompts.Count) -ForegroundColor Green

  # 1. Everything in the environment (a build server), nobody to answer: sent, and nothing asked.
  $null = Use-Data 'environment'
  $env:MOSAIK_STORE_TENANT = 'test-tenant'
  $env:MOSAIK_STORE_CLIENT = 'test-client'
  $env:MOSAIK_STORE_KEY = 'test-key'
  $sent = Send-ToStandin 'everything in the environment' '' 'test-tenant' 'test-client'
  if ((Test-Asked $sent.Lines) -or $sent.Said -match 'Tenant ID|Client ID') { throw ("everything in the environment: it asked something: {0}" -f $sent.Said) }
  Write-Host ("publish.ps1 against a stand-in of the Store: {0} checks passing ({1}), nothing asked." -f $wanted.Count, ($sent.Seen -join ', ')) -ForegroundColor Green
  foreach ($n in 'MOSAIK_STORE_TENANT', 'MOSAIK_STORE_CLIENT', 'MOSAIK_STORE_KEY') { [Environment]::SetEnvironmentVariable($n, $null, 'Process') }

  # 1b. The key in .env.local beside Cargo.toml, with a comment and quotes around it as people
  #     write them: sent, nothing asked, nothing kept. The file goes after.
  $envLocal = Join-Path $tree '.env.local'
  Set-Content -Path $envLocal -Value @('# The Store''s key (Partner Center, Mosaik Store publishing)', 'MOSAIK_STORE_KEY="test-key"') -Encoding UTF8
  $keyFile = Use-Data 'env-local'
  try { $sent = Send-ToStandin 'the key in .env.local' '' $api.tenantId $api.clientId }
  finally { Remove-Item -LiteralPath $envLocal -Force }
  if ((Test-Asked $sent.Lines) -or (Test-Path $keyFile)) { throw ("the key in .env.local: it asked, or kept the key: {0}" -f $sent.Said) }
  Write-Host 'publish.ps1 with the key in .env.local: sends, asks nothing, keeps nothing.' -ForegroundColor Green

  # 1c. The version sent before still with the Store (being checked): it is left alone, nothing is
  #     sent, and publish.ps1 says so and ends with 3, not as a failure, so mosaik.sh --deploy
  #     --app still puts the site online; the same line sends the app once that version is out.
  $null = Use-Data 'still-with-the-store'
  $env:MOSAIK_STORE_KEY = 'test-key'
  try { $run = Invoke-Standin 'the version before still with the Store' '' $api.tenantId $api.clientId 'Certification' }
  finally { [Environment]::SetEnvironmentVariable('MOSAIK_STORE_KEY', $null, 'Process') }
  $touched = @($run.Checks | Where-Object { $_.Name -in 'deleted_while_with_the_store', 'created_while_one_is_with_the_store', 'unexpected', 'unauthorized' } | ForEach-Object { "$($_.Name): $($_.Value)" })
  $looked = @($run.Checks | Where-Object { $_.Name -eq 'pending_status_read' -and $_.Value -eq $true }).Count -eq 1
  if ($run.Code -ne 3 -or -not $looked -or $touched.Count -gt 0 -or $run.Said -notmatch 'still checking' -or $run.Said -match 'refused|Sent:') {
    throw ("the version before still with the Store: exit code {0}; looked at it: {1}; {2}; it said: {3}" -f $run.Code, $looked, ($touched -join ', '), ($run.Lines -join ' / '))
  }
  Write-Host 'publish.ps1 while the version before is still with the Store: leaves it alone, sends nothing, says so, and ends with 3 (not a failure).' -ForegroundColor Green

  # 2. The first run without a key: the key alone asked once, with identity.json's tenant and
  #    client, kept for this user, and sent.
  $keyFile = Use-Data 'first-run'
  $sent = Send-ToStandin 'the first run, the key given' 'test-key' $api.tenantId $api.clientId
  if (-not (Test-Path $keyFile)) { throw "the first run, the key given: it was not kept ($keyFile)" }
  if (-not (Test-Asked $sent.Lines) -or $sent.Said -match 'Tenant ID|Client ID') { throw ("the first run, the key given: not the key alone asked for: {0}" -f $sent.Said) }
  if ((Get-Content -Raw $keyFile) -match 'test-key') { throw "the first run, the key given: kept as it was typed ($keyFile)" }
  # 3. The next run: the kept key, nothing asked.
  $sent = Send-ToStandin 'the next run' '' $api.tenantId $api.clientId
  if ((Test-Asked $sent.Lines) -or $sent.Said -match 'Tenant ID|Client ID') { throw ("the next run: it asked something: {0}" -f $sent.Said) }
  Write-Host ("publish.ps1 the first time without a key: asks for the key alone ({0}, client {1} from identity.json), keeps it encrypted, sends; the next time it sends without asking." -f $api.tenant, $api.clientId) -ForegroundColor Green

  # 4. No key given: nothing kept, nothing sent, a failure that says what is needed, so
  #    mosaik.sh --deploy --app puts nothing online either. Nothing listens where the services
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

  # 6. -KeyOnly, as mosaik.ps1 publish runs it first, so its tests and build need nobody: the
  #    first time the key alone asked, kept, and nothing sent; the next time nothing asked; without
  #    a key, a failure. Nothing listens where the services would be: a request would be an error.
  $keyFile = Use-Data 'key-only'
  foreach ($round in @(@{ Name = 'the first time'; Answers = 'test-key'; Asked = $true; Code = 0 }, @{ Name = 'the next time'; Answers = ''; Asked = $false; Code = 0 })) {
    $out = @($round.Answers | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -KeyOnly 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE
    $said = $out -join ' '
    if ($code -ne $round.Code -or (Test-Asked $out) -ne $round.Asked -or -not (Test-Path $keyFile) -or $said -notmatch 'key is ready' -or $said -match 'Starting a submission|did not give access|refused') {
      throw ("publish.ps1 -KeyOnly, {0}: exit code {1}; it said: {2}" -f $round.Name, $code, ($out -join ' / '))
    }
  }
  $null = Use-Data 'key-only-none'
  $out = @('' | & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publish -KeyOnly 2>&1 | ForEach-Object { "$_" })
  if ($LASTEXITCODE -eq 0 -or ($out -join ' ') -notmatch 'The key is needed') { throw ("publish.ps1 -KeyOnly without a key: exit code {0}; it said: {1}" -f $LASTEXITCODE, ($out -join ' / ')) }
  Write-Host 'publish.ps1 -KeyOnly: asks for the key alone and keeps it, sending nothing; then asks nothing; without a key, ends as a failure.' -ForegroundColor Green

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
