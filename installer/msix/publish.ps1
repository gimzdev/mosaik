#Requires -Version 5.1
<#
.SYNOPSIS
  Sends Mosaik's Store package to the Microsoft Store: a new submission with the package and
  "What's new in this version", for certification. Nothing to answer once this PC has the
  Store's keys.

.DESCRIPTION
  The Microsoft Store submission API (manage.devcenter.microsoft.com) does it the way Partner
  Center takes a submission: a copy of the last published one, with this package in place of the
  old one and "What's new" from whats-new.txt (beside this script); nothing else of the listing
  changes. The Store then checks it, usually within a day, and updates everyone.

  The keys are those of a Microsoft Entra application of the Partner Center account
  (docs/RELEASE.md): asked the first time and kept for this Windows user only (encrypted with
  DPAPI, in %APPDATA%\Mosaik\store-publishing.xml), never in the repository.
  MOSAIK_STORE_TENANT, MOSAIK_STORE_CLIENT and MOSAIK_STORE_KEY, when set, are used instead (a
  build server). Without them nothing is sent.

.PARAMETER Package
  The package to send; by default target\msix\Mosaik_<version>.0_x64.msix, as package.ps1 makes
  it for the version in Cargo.toml.

.PARAMETER Yes
  Nobody to answer (a script): never asks; without keys, nothing is sent.

.PARAMETER SetKeys
  Asks for the Entra application's tenant ID, client ID and key again (a new key), keeps them for
  this Windows user, and sends.

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
  [switch]$SetKeys
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
$Keys = Join-Path $env:APPDATA 'Mosaik\store-publishing.xml'

function Say([string]$Text, [string]$Colour = 'Gray') { Write-Host $Text -ForegroundColor $Colour }
function Fail([string]$Text) { throw $Text }

# The version in Cargo.toml ("2.0.0").
function Get-Version {
  $line = Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
  if (-not $line) { Fail 'No version in Cargo.toml.' }
  $line.Matches[0].Groups[1].Value
}

# A line from the person here, or from a script on its input (nothing at its end); a secret is
# not shown as it is typed.
function Read-Answer([string]$Question, [switch]$Secret) {
  if ([Console]::IsInputRedirected) {
    Say $Question
    $line = [Console]::In.ReadLine()
    if ($null -eq $line) { $line = '' }
    if ($Secret) {
      if ($line -eq '') { return New-Object Security.SecureString }
      return ConvertTo-SecureString $line -AsPlainText -Force
    }
    return $line.Trim()
  }
  if ($Secret) { return Read-Host $Question -AsSecureString }
  (Read-Host $Question).Trim()
}

# The Entra application's tenant ID, client ID and key for the Store's API: from the environment,
# the file kept for this user, or asked once (again with -SetKeys) and kept.
function Get-Access {
  if ($env:MOSAIK_STORE_TENANT -and $env:MOSAIK_STORE_CLIENT -and $env:MOSAIK_STORE_KEY) {
    return [pscustomobject]@{
      Tenant = $env:MOSAIK_STORE_TENANT
      Client = $env:MOSAIK_STORE_CLIENT
      Key    = ConvertTo-SecureString $env:MOSAIK_STORE_KEY -AsPlainText -Force
    }
  }
  if (-not $SetKeys -and (Test-Path $Keys)) { return Import-Clixml $Keys }
  if ($Yes) { Fail "This PC has no keys for the Store's API, and -Yes means nobody is here to give them: run it once in a terminal to give them (docs/RELEASE.md), or set MOSAIK_STORE_TENANT, MOSAIK_STORE_CLIENT and MOSAIK_STORE_KEY." }
  Say ''
  Say 'The Store''s keys, asked this once and kept for this Windows user only, encrypted. They are in' Cyan
  Say 'Partner Center: Account settings, User management, Microsoft Entra applications (docs/RELEASE.md).' Cyan
  $tenant = Read-Answer 'Tenant ID'
  $client = if ($tenant) { Read-Answer 'Client ID' } else { '' }
  $key = if ($client) { Read-Answer 'Key' -Secret } else { New-Object Security.SecureString }
  if ($key.Length -eq 0) { Fail 'The tenant ID, client ID and key are all needed: nothing was kept or sent.' }
  $access = [pscustomobject]@{ Tenant = $tenant; Client = $client; Key = $key }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Keys) | Out-Null
  # A SecureString goes into the file encrypted for this Windows user (DPAPI).
  $access | Export-Clixml -Path $Keys
  Say "Kept in $Keys; from now on it sends without asking." DarkGray
  $access
}

# An access token for the Store's API (an hour long).
function Get-Token($Access) {
  $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Access.Key)
  try {
    $body = @{
      grant_type    = 'client_credentials'
      client_id     = $Access.Client
      client_secret = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
      resource      = 'https://manage.devcenter.microsoft.com'
    }
    try {
      $reply = Invoke-RestMethod -Method Post -Uri "$Login/$($Access.Tenant)/oauth2/token" -Body $body -ContentType 'application/x-www-form-urlencoded'
    }
    catch {
      Fail ("Microsoft did not give access with this tenant ID, client ID and key ({0}). Check them in Partner Center (Account settings, User management, Microsoft Entra applications), then give them again: .\installer\msix\publish.ps1 -SetKeys." -f $_.Exception.Message)
    }
    $reply.access_token
  }
  finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
  }
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

  $version = Get-Version
  if (-not $Package) { $Package = Join-Path $Root "target\msix\Mosaik_$version.0_x64.msix" }
  if (-not (Test-Path $Package)) { Fail "There is no package to send ($Package). Make it first: .\mosaik.ps1 package" }
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
    Say ("  A submission not sent yet was waiting ({0}); it is replaced by this one." -f $pending.id) Yellow
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
  Remove-Item $zip -ErrorAction SilentlyContinue
  Compress-Archive -Path $Package -DestinationPath $zip
  try {
    $null = Invoke-WebRequest -Method Put -Uri $upload -InFile $zip -Headers @{ 'x-ms-blob-type' = 'BlockBlob' } -ContentType 'application/zip' -UseBasicParsing
  }
  catch { Fail ("The package could not be uploaded: {0}" -f $_.Exception.Message) }
  finally { Remove-Item $zip -ErrorAction SilentlyContinue }

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
