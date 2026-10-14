#Requires -Version 5.1
<#
.SYNOPSIS
  Sends Mosaik's Store package to the Microsoft Store: a new submission with the package and
  "What's new in this version", for certification. Nothing to answer once this PC has the key.

.DESCRIPTION
  The Microsoft Store submission API (manage.devcenter.microsoft.com) does it the way Partner
  Center takes a submission: a copy of the last published one, with this package in place of the
  old one and "What's new" from whats-new.txt (beside this script); nothing else of the listing
  changes. The Store then checks it, usually within a day or two, and updates everyone. While it
  still checks the version sent before, it takes no other: nothing is sent, and the exit code is 3
  (0 once sent, 1 on a failure), so a script can tell waiting from failing.

  It signs in as the Partner Center account's Microsoft Entra application, "Mosaik Store
  publishing" (docs/RELEASE.md): its tenant and client IDs are in identity.json (they are not
  secrets). Its key is read, in this order, from MOSAIK_STORE_KEY, from .env.local beside
  Cargo.toml (a line MOSAIK_STORE_KEY=..., in a file git ignores, as in a web project), or from
  what was given once at the prompt and kept for this Windows user only (encrypted with DPAPI, in
  %APPDATA%\Mosaik\store-key.xml). It is never in a file of the repository. MOSAIK_STORE_TENANT
  and MOSAIK_STORE_CLIENT, when set, are used instead of identity.json's. Without the key nothing
  is sent.

.PARAMETER Package
  The package to send; by default target\msix\Mosaik_<version>.0_x64.msix, as package.ps1 makes
  it for the version in Cargo.toml.

.PARAMETER Yes
  Nobody to answer (a script): never asks; without the key, nothing is sent.

.PARAMETER NewKey
  Asks for the key again (a new one from Partner Center, when the kept one has expired), keeps it
  for this Windows user, and sends.

.PARAMETER KeyOnly
  Only makes sure this PC has the key: asks for it when none is kept, keeps it, and stops without
  sending. mosaik.ps1 publish runs this first, so its tests and build need nobody.

.NOTES
  MOSAIK_STORE_API and MOSAIK_STORE_LOGIN point it at a stand-in of Microsoft's services, for
  test-publish.ps1.

.EXAMPLE
  .\installer\msix\publish.ps1
#>
[CmdletBinding()]
param(
  [string]$Package,
  [switch]$Yes,
  [switch]$NewKey,
  [switch]$KeyOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

$Here = $PSScriptRoot
$Root = (Resolve-Path (Join-Path $Here '..\..')).Path
$Identity = Get-Content -Raw -Encoding UTF8 (Join-Path $Here 'identity.json') | ConvertFrom-Json
$StoreId = $Identity.storeId
# The services, as Microsoft documents them; a test points these at a stand-in.
$Api = if ($env:MOSAIK_STORE_API) { $env:MOSAIK_STORE_API.TrimEnd('/') } else { 'https://manage.devcenter.microsoft.com/v1.0/my' }
$Login = if ($env:MOSAIK_STORE_LOGIN) { $env:MOSAIK_STORE_LOGIN.TrimEnd('/') } else { 'https://login.microsoftonline.com' }
$KeyFile = Join-Path $env:APPDATA 'Mosaik\store-key.xml'

function Say([string]$Text, [string]$Colour = 'Gray') { Write-Host $Text -ForegroundColor $Colour }
function Fail([string]$Text) { throw $Text }

# The workspace version in Cargo.toml ("1.0.2"), as package.ps1 reads it.
function Get-Version {
  $cargo = Get-Content -Raw -Encoding UTF8 (Join-Path $Root 'Cargo.toml')
  $match = [regex]::Match($cargo, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
  if (-not $match.Success) { Fail 'The version was not found in Cargo.toml ([workspace.package] version).' }
  $match.Groups[1].Value
}

# A secure string as plain text, for as long as it is needed.
function ConvertFrom-Secure([Security.SecureString]$Secure) {
  if ($null -eq $Secure -or $Secure.Length -eq 0) { return '' }
  $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
  try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
  finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

# The key, without the spaces and line breaks around it; $null when none was given. Asked at a
# hidden prompt, or read from a script on its input (`cat /dev/clipboard | ...` in Git Bash). When
# nothing comes through the prompt (Git Bash's window does not pass a paste on to it), the key
# copied to the clipboard is taken; it is asked three times at most.
function Read-Key([bool]$Console = -not [Console]::IsInputRedirected) {
  for ($try = 1; $try -le 3; $try++) {
    if ($Console) {
      $key = (ConvertFrom-Secure (Read-Host 'Key (paste it, or copy it and press Enter)' -AsSecureString)).Trim()
      if (-not $key) {
        $copied = try { "$(Get-Clipboard -Raw)".Trim() } catch { '' }
        # A key is one word: anything else on the clipboard is not it.
        if ($copied -match '^\S{16,256}$') {
          $key = $copied
          Say '  Taken from the clipboard.' DarkGray
        }
      }
    }
    else {
      if ($try -eq 1) { Say 'Key' }
      $line = [Console]::In.ReadLine()
      if ($null -eq $line) { break }
      $key = $line.Trim()
    }
    if ($key) { return ConvertTo-SecureString $key -AsPlainText -Force }
    if ($Console -and $try -lt 3) { Say '  Nothing came through: copy the key, then press Enter.' Yellow }
  }
  $null
}

# The settings of this PC that never go in the repository: .env.local beside Cargo.toml, which git
# ignores, with KEY=value lines as in a web project (comments, blank lines and quotes allowed).
function Read-LocalEnv {
  $values = @{}
  $file = Join-Path $Root '.env.local'
  if (Test-Path -LiteralPath $file) {
    foreach ($line in Get-Content -LiteralPath $file -Encoding UTF8) {
      if ($line -match '^\s*(?:export\s+)?([A-Za-z_]\w*)\s*=\s*(.*?)\s*$') {
        $name = $Matches[1]
        $value = $Matches[2]
        if ($value -match '^(["''])(.*)\1$') { $value = $Matches[2] }
        $values[$name] = $value
      }
    }
  }
  $values
}

# The Entra application's tenant ID, client ID and key for the Store's API: the IDs from the
# environment or identity.json; the key from the environment, .env.local, the file kept for this
# user, or asked once (again with -NewKey) and kept.
function Get-Access {
  $api = $Identity.storeApi
  $tenant = if ($env:MOSAIK_STORE_TENANT) { $env:MOSAIK_STORE_TENANT } else { $api.tenantId }
  $client = if ($env:MOSAIK_STORE_CLIENT) { $env:MOSAIK_STORE_CLIENT } else { $api.clientId }
  $access = { param($Key) [pscustomobject]@{ Tenant = $tenant; Client = $client; Key = $Key } }
  if ($env:MOSAIK_STORE_KEY) { return & $access (ConvertTo-SecureString $env:MOSAIK_STORE_KEY -AsPlainText -Force) }
  $local = (Read-LocalEnv)['MOSAIK_STORE_KEY']
  if (-not $NewKey -and $local) { return & $access (ConvertTo-SecureString $local -AsPlainText -Force) }
  if (-not $NewKey -and (Test-Path -LiteralPath $KeyFile)) {
    # Encrypted for the Windows user who gave it: another account cannot read it.
    try { return & $access (Import-Clixml -LiteralPath $KeyFile) }
    catch { Fail ("The Store's key kept in {0} cannot be read by this Windows user ({1}). Give it again: .\installer\msix\publish.ps1 -NewKey" -f $KeyFile, $_.Exception.Message) }
  }
  $envFile = Join-Path $Root '.env.local'
  if ($Yes) { Fail ("This PC has no key for the Store's API, and -Yes means nobody is here to give it: put it in {0} as MOSAIK_STORE_KEY=... (docs/RELEASE.md), or set MOSAIK_STORE_KEY." -f $envFile) }
  Say ''
  Say ("The Store's key: put it in {0}, on a line MOSAIK_STORE_KEY=... (git ignores that file)," -f $envFile) Cyan
  Say 'or give it here once, kept encrypted for this Windows user. Partner Center makes one in' Cyan
  Say 'Account settings, User management, Microsoft Entra applications, Mosaik Store publishing.' Cyan
  $key = Read-Key
  if ($null -eq $key) { Fail 'The key is needed: nothing was kept or sent.' }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $KeyFile) | Out-Null
  # A SecureString goes into the file encrypted for this Windows user (DPAPI).
  Export-Clixml -LiteralPath $KeyFile -InputObject $key
  Say "Kept in $KeyFile; from now on it sends without asking." DarkGray
  & $access $key
}

# An access token for the Store's API (an hour long).
function Get-Token($Access) {
  $body = @{
    grant_type    = 'client_credentials'
    client_id     = $Access.Client
    client_secret = ConvertFrom-Secure $Access.Key
    resource      = 'https://manage.devcenter.microsoft.com'
  }
  try {
    $reply = Invoke-RestMethod -Method Post -Uri "$Login/$($Access.Tenant)/oauth2/token" -Body $body -ContentType 'application/x-www-form-urlencoded'
  }
  catch {
    Fail ("Microsoft did not give access with this key ({0}). Partner Center shows the key's dates, and makes a new one, in Account settings, User management, Microsoft Entra applications, Mosaik Store publishing; give it with: .\installer\msix\publish.ps1 -NewKey" -f $_.Exception.Message)
  }
  $reply.access_token
}

# A call to the Store's API for this app; the body, when there is one, as JSON.
function Invoke-Store([string]$Method, [string]$Path, $Body = $null) {
  $call = @{
    Method      = $Method
    Uri         = "$Api/applications/$StoreId$Path"
    Headers     = @{ Authorization = "Bearer $script:Token" }
    ContentType = 'application/json; charset=utf-8'
  }
  if ($null -ne $Body) { $call.Body = [Text.Encoding]::UTF8.GetBytes(($Body | ConvertTo-Json -Depth 64)) }
  try { Invoke-RestMethod @call }
  catch {
    $detail = $_.ErrorDetails.Message
    Fail ("The Store refused {0} {1}: {2}{3}" -f $Method, $Path, $_.Exception.Message, $(if ($detail) { " ($detail)" } else { '' }))
  }
}

# Sets a property of an object read from JSON, adding it when the object has none.
function Set-Field($Object, [string]$Name, $Value) {
  if ($Object.PSObject.Properties[$Name]) { $Object.$Name = $Value }
  else { $Object | Add-Member -NotePropertyName $Name -NotePropertyValue $Value }
}

try {
  # TLS 1.2 for Windows PowerShell 5.1 on older .NET defaults.
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
  # Windows PowerShell 5.1 writes some arrays as {"value": [...], "Count": n} because of the type
  # data it adds to arrays; without it, they are plain JSON arrays (this session only).
  Remove-TypeData -TypeName System.Array -ErrorAction SilentlyContinue

  if ($KeyOnly) {
    $null = Get-Access
    Say "The Store's key is ready: sending asks nothing." Green
    exit 0
  }

  $version = Get-Version
  if (-not $Package) { $Package = Join-Path $Root "target\msix\Mosaik_$version.0_x64.msix" }
  if (-not (Test-Path -LiteralPath $Package)) { Fail "There is no package to send ($Package). Make it first: .\mosaik.ps1 package" }
  $notesFile = Join-Path $Here 'whats-new.txt'
  $notes = if (Test-Path $notesFile) { (Get-Content -Raw -Encoding UTF8 $notesFile).Trim() } else { '' }
  if (-not $notes) { Fail "Write what is new in $version in $notesFile first: the Store shows it as `"What's new in this version`"." }

  $access = Get-Access
  Say ''
  Say ("Mosaik {0} to the Microsoft Store (app {1}): {2}" -f $version, $StoreId, (Split-Path -Leaf $Package)) Cyan
  Say "What's new: $notes" DarkGray

  $script:Token = Get-Token $access
  Say '  Starting a submission (a copy of the last one published)' DarkGray
  $app = Invoke-Store GET ''
  $pending = if ($app.PSObject.Properties['pendingApplicationSubmission']) { $app.pendingApplicationSubmission } else { $null }
  if ($pending -and $pending.id) {
    $before = (Invoke-Store GET "/submissions/$($pending.id)/status").status
    # Sent and not out yet: the Store keeps it until it is, and takes no other meanwhile.
    if ($before -in 'CommitStarted', 'PreProcessing', 'Certification', 'Release', 'PendingPublication', 'Publishing') {
      Say ("The Store is still checking the version sent before ({0}): Mosaik {1} waits for it. Run the same line again once it is out, usually within a day or two. Nothing was sent." -f $before, $version) Yellow
      exit 3
    }
    Say ("  A submission waiting in Partner Center ({0}, {1}) is replaced by this one." -f $pending.id, $before) Yellow
    $null = Invoke-Store DELETE "/submissions/$($pending.id)"
  }
  $submission = Invoke-Store POST '/submissions'

  # This package in place of the ones before; "What's new" in every language of the listing.
  $name = Split-Path -Leaf $Package
  $packages = @(@($submission.applicationPackages) | Where-Object { $null -ne $_ } | ForEach-Object { Set-Field $_ 'fileStatus' 'PendingDelete'; $_ })
  $packages += [pscustomobject]@{ fileName = $name; fileStatus = 'PendingUpload'; minimumDirectXVersion = 'None'; minimumSystemRam = 'None' }
  Set-Field $submission 'applicationPackages' $packages
  $languages = @($submission.listings.PSObject.Properties)
  if ($languages.Count -eq 0) { Fail 'The submission has no listing to put "What''s new" in.' }
  foreach ($language in $languages) { Set-Field $language.Value.baseListing 'releaseNotes' $notes }
  $upload = $submission.fileUploadUrl
  $id = $submission.id
  $null = Invoke-Store PUT "/submissions/$id" $submission

  Say '  Uploading the package' DarkGray
  $zip = Join-Path ([IO.Path]::GetTempPath()) "mosaik-store-$version-$PID.zip"
  Remove-Item -LiteralPath $zip -ErrorAction SilentlyContinue
  Compress-Archive -LiteralPath $Package -DestinationPath $zip
  try {
    $null = Invoke-WebRequest -Method Put -Uri $upload -InFile $zip -Headers @{ 'x-ms-blob-type' = 'BlockBlob' } -ContentType 'application/zip' -UseBasicParsing
  }
  catch { Fail ("The package could not be uploaded: {0}" -f $_.Exception.Message) }
  finally { Remove-Item -LiteralPath $zip -ErrorAction SilentlyContinue }

  Say '  Sending it for certification' DarkGray
  $null = Invoke-Store POST "/submissions/$id/commit"
  $watch = [Diagnostics.Stopwatch]::StartNew()
  do {
    Start-Sleep -Seconds $(if ($env:MOSAIK_STORE_API) { 1 } else { 10 })
    $status = Invoke-Store GET "/submissions/$id/status"
  } while ($status.status -eq 'CommitStarted' -and $watch.Elapsed.TotalMinutes -lt 10)
  if ($status.status -in 'CommitFailed', 'PreProcessingFailed') {
    $errors = @()
    if ($status.PSObject.Properties['statusDetails'] -and $status.statusDetails) {
      $errors = @($status.statusDetails.errors | ForEach-Object { "$($_.code): $($_.details)" })
    }
    Fail ("The Store did not take the submission ({0}): {1}. It waits in Partner Center, where it can be fixed and sent." -f $status.status, ($errors -join '; '))
  }
  Say ("Sent: Mosaik {0} is with the Store ({1}). It is usually checked within a day or two; then everyone gets it." -f $version, $status.status) Green
  exit 0
}
catch {
  Write-Host $_.Exception.Message -ForegroundColor Red
  exit 1
}
