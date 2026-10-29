#Requires -Version 5.1
<#
.SYNOPSIS
  Sends Mosaik's Store package to the Microsoft Store: a new submission with the package, "What's
  new in this version" and, when listing\ is beside this script, the Store listing written there
  (description, short description, features, search terms, screenshots), for certification.
  Nothing to answer once this PC has the key.

.DESCRIPTION
  The Microsoft Store submission API (manage.devcenter.microsoft.com) does it the way Partner
  Center takes a submission: a copy of the last published one, with this package in place of the
  old one and "What's new" from whats-new.txt (beside this script). The rest of the listing changes
  only where listing\<language>\ beside this script says so (en-us, for one): description.txt
  replaces the description; short-description.txt the short description shown at the top of the
  listing; features.txt and keywords.txt, one per line, the features and the search
  terms; the PNG files in screenshots\, in the order of their names, the screenshots, captioned by
  the lines of screenshots\captions.txt in the same order. A part without its file stays as the
  Store has it; without listing\, the listing is not touched. The Store then checks it, usually
  within a day or two, and updates everyone. While it
  still checks the version sent before, it takes no other: nothing is sent, and the exit code is 3
  (0 once sent, 1 on a failure), so a script can tell waiting from failing. A version the Store
  has already, or an older one, is never sent: it takes a version only once, even built again
  from the same source, and an older one would reach nobody.

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
  sending.

.PARAMETER Check
  Asks the Store first, sending nothing: what mosaik.ps1 publish runs before its tests and build,
  so they need nobody and are made only for a version the Store takes. The key (asked for when
  none is kept, as with -KeyOnly), "What's new" and the listing are checked, then the Store is
  asked. The exit code: 0 when the Store takes this version; 3 while it still checks a version sent
  before (nothing to do yet); 4 when the one it checks is this version (sent already: nothing to
  do); 1 when it has this version already or a newer one (2026-10-28: the 1.0.3 built again was
  tested, built and refused at the end), or on any other failure.

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
  [switch]$KeyOnly,
  [switch]$Check
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
$ListingDir = Join-Path $Here 'listing'
# A submission sent and not out yet: the Store keeps it until it is, and takes no other meanwhile.
$InFlight = 'CommitStarted', 'PreProcessing', 'Certification', 'Release', 'PendingPublication', 'Publishing'

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

# A text file's lines, without the spaces around them; blank lines left out unless -Blank.
function Read-Lines([string]$File, [switch]$Blank) {
  @(Get-Content -LiteralPath $File -Encoding UTF8 | ForEach-Object { "$_".Trim() } | Where-Object { $Blank -or $_ })
}

# The listing in listing\<language>\, one entry per language: the parts given ($null for a part
# left as the Store has it) and the screenshots, each with its name in the ZIP (at its root, as the
# package is: "<language>-<file>") and its caption. Checked against the Store's limits first, so a
# listing that cannot go sends nothing.
function Get-Listing {
  $languages = @()
  if (-not (Test-Path -LiteralPath $ListingDir)) { return ,$languages }
  foreach ($dir in @(Get-ChildItem -LiteralPath $ListingDir -Directory | Sort-Object Name)) {
    $language = $dir.Name.ToLowerInvariant()
    $description = $null; $short = $null; $features = $null; $keywords = $null; $shots = $null
    $file = Join-Path $dir.FullName 'description.txt'
    if (Test-Path -LiteralPath $file) {
      $description = "$(Get-Content -Raw -Encoding UTF8 -LiteralPath $file)".Trim()
      if (-not $description) { Fail "$file is empty: write the description, or remove the file to keep the Store's." }
      if ($description.Length -gt 10000) { Fail "$file is $($description.Length) characters long; the Store takes 10,000 at most." }
    }
    # The short description, at the top of the listing (the Store shows its first 270 characters in
    # some views)
    $file = Join-Path $dir.FullName 'short-description.txt'
    if (Test-Path -LiteralPath $file) {
      $short = "$(Get-Content -Raw -Encoding UTF8 -LiteralPath $file)".Trim()
      if (-not $short) { Fail "$file is empty: write the short description, or remove the file to keep the Store's." }
      if ($short.Length -gt 1000) { Fail "$file is $($short.Length) characters long; the Store takes 1,000 at most." }
    }
    $file = Join-Path $dir.FullName 'features.txt'
    if (Test-Path -LiteralPath $file) {
      $features = @(Read-Lines $file)
      if ($features.Count -gt 20) { Fail "$file has $($features.Count) features; the Store takes 20 at most." }
      $long = @($features | Where-Object { $_.Length -gt 200 })
      if ($long.Count -gt 0) { Fail "$file has a feature over 200 characters, which the Store refuses: $($long[0])" }
    }
    $file = Join-Path $dir.FullName 'keywords.txt'
    if (Test-Path -LiteralPath $file) {
      $keywords = @(Read-Lines $file)
      if ($keywords.Count -gt 7) { Fail "$file has $($keywords.Count) search terms; the Store takes 7 at most." }
    }
    $folder = Join-Path $dir.FullName 'screenshots'
    if (Test-Path -LiteralPath $folder) {
      $files = @(Get-ChildItem -LiteralPath $folder -File -Filter '*.png' | Sort-Object Name)
      if ($files.Count -gt 10) { Fail "$folder has $($files.Count) screenshots; the Store takes 10 at most." }
      if ($files.Count -gt 0) {
        $captionsFile = Join-Path $folder 'captions.txt'
        # Assigned apart from the if: an if's output of one line would be a string, not a list
        $captions = @()
        if (Test-Path -LiteralPath $captionsFile) { $captions = @(Read-Lines $captionsFile -Blank) }
        $shots = @()
        for ($i = 0; $i -lt $files.Count; $i++) {
          $caption = if ($i -lt $captions.Count) { $captions[$i] } else { '' }
          if ($caption.Length -gt 200) { Fail "$captionsFile, line $($i + 1): over 200 characters, which the Store refuses." }
          $shots += [pscustomobject]@{ Path = $files[$i].FullName; FileName = ('{0}-{1}' -f $language, $files[$i].Name); Caption = $caption }
        }
      }
    }
    $languages += [pscustomobject]@{ Language = $language; Description = $description; Short = $short; Features = $features; Keywords = $keywords; Screenshots = $shots }
  }
  ,$languages
}

# Puts the listing read by Get-Listing in the submission; out come the screenshots it adds, to go in the ZIP.
function Set-Listing($Submission, $Listing) {
  $added = @()
  foreach ($l in $Listing) {
    $entry = @($Submission.listings.PSObject.Properties | Where-Object { $_.Name -ieq $l.Language }) | Select-Object -First 1
    if (-not $entry) {
      Say ("  listing\{0} left out: the Store's listing has no {0} (the package must declare that language)." -f $l.Language) Yellow
      continue
    }
    $base = $entry.Value.baseListing
    $parts = @()
    if ($null -ne $l.Description) { Set-Field $base 'description' $l.Description; $parts += 'description' }
    if ($null -ne $l.Short) { Set-Field $base 'shortDescription' $l.Short; $parts += 'short description' }
    if ($null -ne $l.Features) { Set-Field $base 'features' @($l.Features); $parts += "$($l.Features.Count) features" }
    if ($null -ne $l.Keywords) { Set-Field $base 'keywords' @($l.Keywords); $parts += "$($l.Keywords.Count) search terms" }
    if ($null -ne $l.Screenshots) {
      # The screenshots there before go; the other pictures (logos, promotional art) stay.
      $before = @()
      if ($base.PSObject.Properties['images']) { $before = @(@($base.images) | Where-Object { $null -ne $_ }) }
      $images = @($before | ForEach-Object {
          if ($_.PSObject.Properties['imageType'] -and $_.imageType -eq 'Screenshot') { Set-Field $_ 'fileStatus' 'PendingDelete' }
          $_
        })
      foreach ($shot in $l.Screenshots) {
        $images += [pscustomobject]@{ fileName = $shot.FileName; fileStatus = 'PendingUpload'; description = $shot.Caption; imageType = 'Screenshot' }
        $added += $shot
      }
      Set-Field $base 'images' $images
      $parts += "$($l.Screenshots.Count) screenshots"
    }
    if ($parts.Count -gt 0) { Say ("  Listing ({0}): {1}, from listing\{0}" -f $l.Language, ($parts -join ', ')) DarkGray }
  }
  $added
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

# The versions of the packages a submission has (as [version]: 1.0.3.0), the ones it removes left
# out; none when it has none yet, or the Store gives none.
function Get-PackageVersions($Submission) {
  $packages = @()
  if ($Submission -and $Submission.PSObject.Properties['applicationPackages']) { $packages = @(@($Submission.applicationPackages) | Where-Object { $null -ne $_ }) }
  foreach ($p in $packages) {
    if ($p.PSObject.Properties['fileStatus'] -and $p.fileStatus -eq 'PendingDelete') { continue }
    if (-not $p.PSObject.Properties['version']) { continue }
    $v = $null
    if ([version]::TryParse([string]$p.version, [ref]$v)) { $v }
  }
}

# A package's version as Cargo.toml writes it (1.0.3.0 is 1.0.3).
function Format-Version([version]$Version) {
  if ($Version.Revision -le 0 -and $Version.Build -ge 0) { '{0}.{1}.{2}' -f $Version.Major, $Version.Minor, $Version.Build }
  else { "$Version" }
}

# Where the Store stands for $Version (Cargo.toml's; its package is $Version.0). State: 'go' when
# it takes it; 'wait' while it still checks a submission sent before, and 'sent' when that one is
# this version. Pending: a submission not sent (left in Partner Center, or refused), which the
# next one replaces; Published: the version the Store has. A failure when the Store has this
# version already or a newer one: it takes a version only once, even built again from the same
# source (2026-10-28: "two packages with the full name Gimzdev.Mosak_1.0.3.0_X64_ which have
# different contents"), and an older one would reach nobody.
function Get-StoreState([string]$Version) {
  $new = [version]"$Version.0"
  $state = [pscustomobject]@{ State = 'go'; Status = ''; Pending = ''; Published = '' }
  $app = Invoke-Store GET ''
  $pending = if ($app.PSObject.Properties['pendingApplicationSubmission']) { $app.pendingApplicationSubmission } else { $null }
  if ($pending -and $pending.PSObject.Properties['id'] -and $pending.id) {
    $state.Status = "$((Invoke-Store GET "/submissions/$($pending.id)/status").status)"
    if ($state.Status -in $InFlight) {
      # Its version only says which it is: when it cannot be read, it is the version sent before
      $sent = @()
      try { $sent = @(Get-PackageVersions (Invoke-Store GET "/submissions/$($pending.id)")) }
      catch { Write-Verbose "The submission with the Store was not read: $($_.Exception.Message)" }
      $state.State = if ($sent -contains $new) { 'sent' } else { 'wait' }
      return $state
    }
    $state.Pending = "$($pending.id)"
  }
  $last = if ($app.PSObject.Properties['lastPublishedApplicationSubmission']) { $app.lastPublishedApplicationSubmission } else { $null }
  if ($last -and $last.PSObject.Properties['id'] -and $last.id) {
    # Not read, the version the Store has stops nothing: the Store itself refuses one it has
    $versions = @()
    try { $versions = @(Get-PackageVersions (Invoke-Store GET "/submissions/$($last.id)") | Sort-Object -Descending) }
    catch { Say ("  The version the Store has could not be read ({0}); it refuses this one itself if it has it." -f $_.Exception.Message) Yellow }
    if ($versions.Count -gt 0) {
      $state.Published = Format-Version $versions[0]
      if ($new -le $versions[0]) {
        Fail ("The Store has Mosaik {0} already: it takes a version only once, even built again from the same source, and an older one would reach nobody. Give Mosaik its next version first (docs/RELEASE.md says where), then run this again. Nothing was sent." -f $state.Published)
      }
    }
  }
  $state
}

# What is said when the Store still checks a submission sent before, or has this version already.
function Show-Waiting($State, [string]$Version, [string]$Nothing) {
  if ($State.State -eq 'sent') {
    $text = "Mosaik {0} is with the Store already ({1}): nothing to send. It is usually checked within a day or two; then everyone gets it. {2}" -f $Version, $State.Status, $Nothing
    Say $text.TrimEnd() Green
  }
  else {
    $text = "The Store is still checking the version sent before ({0}): Mosaik {1} waits for it. Run the same line again once it is out, usually within a day or two. {2}" -f $State.Status, $Version, $Nothing
    Say $text.TrimEnd() Yellow
  }
}

try {
  # TLS 1.2 for Windows PowerShell 5.1 on older .NET defaults.
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
  # Windows PowerShell 5.1 writes some arrays as {"value": [...], "Count": n} because of the type
  # data it adds to arrays; without it, they are plain JSON arrays (this session only).
  Remove-TypeData -TypeName System.Array -ErrorAction SilentlyContinue

  if ($KeyOnly) {
    # The listing checked too, so one the Store would refuse stops mosaik.ps1 publish before its build
    $null = Get-Listing
    $null = Get-Access
    Say "The Store's key is ready: sending asks nothing." Green
    exit 0
  }

  $version = Get-Version
  $notesFile = Join-Path $Here 'whats-new.txt'
  $notes = if (Test-Path $notesFile) { (Get-Content -Raw -Encoding UTF8 $notesFile).Trim() } else { '' }
  if (-not $notes) { Fail "Write what is new in $version in $notesFile first: the Store shows it as `"What's new in this version`"." }
  $listing = Get-Listing

  if ($Check) {
    # Before mosaik.ps1 publish tests and builds: the Store is asked, and nothing is sent.
    $script:Token = Get-Token (Get-Access)
    $state = Get-StoreState $version
    if ($state.State -ne 'go') {
      Show-Waiting $state $version 'Nothing was tested, built or sent.'
      exit $(if ($state.State -eq 'sent') { 4 } else { 3 })
    }
    $has = if ($state.Published) { " (it has $($state.Published))" } else { '' }
    Say ("The Store takes Mosaik {0}{1}, and its key is ready: sending asks nothing." -f $version, $has) Green
    exit 0
  }

  if (-not $Package) { $Package = Join-Path $Root "target\msix\Mosaik_$version.0_x64.msix" }
  if (-not (Test-Path -LiteralPath $Package)) { Fail "There is no package to send ($Package). Make it first: .\mosaik.ps1 package" }

  $access = Get-Access
  Say ''
  Say ("Mosaik {0} to the Microsoft Store (app {1}): {2}" -f $version, $StoreId, (Split-Path -Leaf $Package)) Cyan
  Say "What's new: $notes" DarkGray

  $script:Token = Get-Token $access
  Say '  Starting a submission (a copy of the last one published)' DarkGray
  $state = Get-StoreState $version
  if ($state.State -ne 'go') {
    # This very version with the Store already: done, as when it is sent
    Show-Waiting $state $version $(if ($state.State -eq 'sent') { '' } else { 'Nothing was sent.' })
    exit $(if ($state.State -eq 'sent') { 0 } else { 3 })
  }
  if ($state.Pending) {
    Say ("  A submission waiting in Partner Center ({0}, {1}) is replaced by this one." -f $state.Pending, $state.Status) Yellow
    $null = Invoke-Store DELETE "/submissions/$($state.Pending)"
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
  $shots = @(Set-Listing $submission $listing)
  $upload = $submission.fileUploadUrl
  $id = $submission.id
  $null = Invoke-Store PUT "/submissions/$id" $submission

  Say $(if ($shots.Count -gt 0) { "  Uploading the package and $($shots.Count) screenshots" } else { '  Uploading the package' }) DarkGray
  $zip = Join-Path ([IO.Path]::GetTempPath()) "mosaik-store-$version-$PID.zip"
  # The screenshots under their names in the ZIP, at its root: no folders, whose separators
  # Windows PowerShell's Compress-Archive writes as backslashes.
  $stage = Join-Path ([IO.Path]::GetTempPath()) "mosaik-store-$version-$PID"
  Remove-Item -LiteralPath $zip -ErrorAction SilentlyContinue
  try {
    $inZip = @($Package)
    if ($shots.Count -gt 0) {
      New-Item -ItemType Directory -Force -Path $stage | Out-Null
      foreach ($shot in $shots) {
        $copy = Join-Path $stage $shot.FileName
        Copy-Item -LiteralPath $shot.Path -Destination $copy
        $inZip += $copy
      }
    }
    Compress-Archive -LiteralPath $inZip -DestinationPath $zip
    $null = Invoke-WebRequest -Method Put -Uri $upload -InFile $zip -Headers @{ 'x-ms-blob-type' = 'BlockBlob' } -ContentType 'application/zip' -UseBasicParsing
  }
  catch { Fail ("The package could not be uploaded: {0}" -f $_.Exception.Message) }
  finally {
    Remove-Item -LiteralPath $zip -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
  }

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
