#Requires -Version 5.1
<#
.SYNOPSIS
  Sends Mosaik's Store package to the Microsoft Store: a new submission with the package and
  "What's new in this version", for certification. By hand in Partner Center, or through the
  Store's API when this PC has its keys.

.DESCRIPTION
  By hand (no keys): Partner Center opens on Mosaik, File Explorer shows the package and "What's
  new" (whats-new.txt, beside this script) is put on the clipboard; you start the update, drop the
  package in, paste "What's new" and submit, then say so here.

  With keys: the Microsoft Store submission API (manage.devcenter.microsoft.com) does it the way
  Partner Center takes a submission: a copy of the last published one, with this package in place
  of the old one and "What's new" from whats-new.txt; nothing else of the listing changes. The
  keys are those of a Microsoft Entra application of the Partner Center account (docs/RELEASE.md,
  "Sending through the Store's API"), given once with -SetKeys and kept for this Windows user only
  (encrypted with DPAPI, in %APPDATA%\Mosaik\store-publishing.xml), never in the repository;
  MOSAIK_STORE_TENANT, MOSAIK_STORE_CLIENT and MOSAIK_STORE_KEY, when set, are used instead (a
  build server).

  Either way the Store then checks it, usually within a day or two, and updates everyone.

.PARAMETER Package
  The package to send; by default target\msix\Mosaik_<version>.0_x64.msix, as package.ps1 makes
  it for the version in Cargo.toml.

.PARAMETER Yes
  Sends through the Store's API without asking first (a script, with keys; a person is asked).

.PARAMETER SetKeys
  Asks for the Entra application's tenant ID, client ID and key, keeps them for this Windows user,
  and sends through the Store's API from then on.

.NOTES
  MOSAIK_STORE_API and MOSAIK_STORE_LOGIN point it at a stand-in of Microsoft's services, and
  MOSAIK_STORE_OPEN=print writes what a send by hand would open instead of opening it: both for
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

# The Entra application's tenant ID, client ID and key for the Store's API: from the environment,
# the file kept for this user, or asked (-SetKeys). None: the package is sent by hand.
function Get-Access {
  if ($env:MOSAIK_STORE_TENANT -and $env:MOSAIK_STORE_CLIENT -and $env:MOSAIK_STORE_KEY) {
    return [pscustomobject]@{
      Tenant = $env:MOSAIK_STORE_TENANT
      Client = $env:MOSAIK_STORE_CLIENT
      Key    = ConvertTo-SecureString $env:MOSAIK_STORE_KEY -AsPlainText -Force
    }
  }
  if (-not $SetKeys) {
    if (Test-Path $Keys) { return Import-Clixml $Keys }
    return $null
  }
  if ([Console]::IsInputRedirected) {
    Fail "-SetKeys asks for the Entra application's tenant ID, client ID and key: run it in a terminal, or set MOSAIK_STORE_TENANT, MOSAIK_STORE_CLIENT and MOSAIK_STORE_KEY."
  }
  Say 'The Microsoft Entra application of your Partner Center account (docs/RELEASE.md, "Sending through' Cyan
  Say 'the Store''s API"). Kept for this Windows user only, encrypted.' Cyan
  $tenant = (Read-Host 'Tenant ID').Trim()
  $client = (Read-Host 'Client ID').Trim()
  $key = Read-Host 'Key' -AsSecureString
  if (-not $tenant -or -not $client -or $key.Length -eq 0) { Fail 'The tenant ID, client ID and key are all needed.' }
  $access = [pscustomobject]@{ Tenant = $tenant; Client = $client; Key = $key }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Keys) | Out-Null
  # A SecureString goes into the file encrypted for this Windows user (DPAPI).
  $access | Export-Clixml -Path $Keys
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
      Fail ("Microsoft did not give access with this tenant ID, client ID and key ({0}). Check them in Partner Center (Account settings, User management, Microsoft Entra applications), then give them again: .\installer\msix\publish.ps1 -SetKeys (or delete {1} to send by hand)." -f $_.Exception.Message, $Keys)
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

# A yes from the person here, or from a script on its input (nothing is a no).
function Get-Yes([string]$Question) {
  $answer = if ([Console]::IsInputRedirected) { Say $Question; [Console]::In.ReadLine() } else { Read-Host $Question }
  "$answer".Trim() -match '^(y|yes|o|oui)$'
}

# By hand in Partner Center: its page for Mosaik opens, File Explorer shows the package, "What's
# new" goes on the clipboard, and the steps are said; then the person says whether it was
# submitted. Not submitted ends as a failure, so mosaik1.sh --deploy --app puts nothing online.
function Send-ByHand([string]$Version, [string]$Path, [string]$Notes) {
  $page = "https://partner.microsoft.com/dashboard/products/$StoreId/overview"
  Say ''
  Say ("Mosaik {0} to the Microsoft Store (app {1}), by hand in Partner Center: this PC has no keys for the Store's API (docs/RELEASE.md)." -f $Version, $StoreId) Cyan
  if ($env:MOSAIK_STORE_OPEN -eq 'print') {
    Say "Would open: $page"
    Say "Would show: $Path"
    Say "Would copy: What's new"
  }
  else {
    Start-Process $page
    Start-Process explorer.exe -ArgumentList "/select,`"$Path`""
    Set-Clipboard -Value $Notes
  }
  Say "What's new (on the clipboard): $Notes" DarkGray
  Say '  1. In Partner Center (it opened on Mosaik, signed in as you): Start update.'
  Say ("  2. Packages: drag in {0} from the folder that opened; remove the older package; Save." -f (Split-Path -Leaf $Path))
  Say '  3. Store listings, English: paste into "What''s new in this version" (it is on the clipboard); Save.'
  Say '  4. Submit it.'
  if (-not (Get-Yes 'Submitted? [y/N]')) { Say 'Nothing was sent.' Yellow; exit 1 }
  Say ("Sent: Mosaik {0} is with the Store. It is usually checked within a day or two; then everyone gets it." -f $Version) Green
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
  if (-not $access) {
    if ($Yes) { Fail "-Yes sends through the Store's API, and this PC has no keys for it: give them once with -SetKeys (docs/RELEASE.md), or send it by hand without -Yes." }
    Send-ByHand $version $Package $notes
    # An exit code of its own: the caller reads it (mosaik.ps1 publish, then mosaik1.sh --app).
    exit 0
  }
  Say ''
  Say ("Mosaik {0} to the Microsoft Store (app {1}): {2}" -f $version, $StoreId, (Split-Path -Leaf $Package)) Cyan
  Say "What's new: $notes" DarkGray
  # A no ends as a failure: mosaik1.sh --deploy --app then puts nothing online either.
  if (-not $Yes -and -not (Get-Yes 'Send it now? The Store checks it, then everyone gets it. [y/N]')) {
    Say 'Nothing was sent.' Yellow
    exit 1
  }

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
