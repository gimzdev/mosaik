# 0041. The Store listing goes with the app

- Status: accepted
- Date: 2026-10-26
- Amends: ADR 0025 (nothing else of the listing changes), docs/RELEASE.md

## Context

The owner, on 2026-10-26 at 09:38, about `./mosaik.sh --app`: "shouldnt the --app publish on store
with screenshots and shit ?". It sent the package and "What's new" alone (ADR 0025): a new
description, features, search terms or screenshots were pasted and uploaded in Partner Center by
hand, and a submission made by the API is better left alone there meanwhile (ADR 0025).

## Decision

**`publish.ps1` also sends the listing written in `installer/msix/listing/<language>/`**, in the
same submission. `description.txt`, `features.txt` and `keywords.txt` (one per line) replace the
description, the features and the search terms; the PNG files in `screenshots/`, in the order of
their names, replace the listing's screenshots, captioned by the lines of `screenshots/captions.txt`,
its other pictures staying. A part without its file stays as the Store has it; without the folder,
the listing is not touched. The screenshots go in the ZIP with the package, at its root, named
`<language>-<file>`: no folders, whose separators Windows PowerShell's `Compress-Archive` writes as
backslashes. The listing is read and checked against the Store's limits (a description of 10,000
characters, 20 features of 200, 7 search terms, 10 screenshots, captions of 200) before the key is
read or anything sent, and by `-KeyOnly` too, which `mosaik.ps1 publish` runs first: a listing the
Store would refuse stops the publish before its build. A folder for a language the Store's listing
has not is left out, saying so.

`listing/en-us` starts with the English description, features and search terms written for the
launch on 2026-10-26, and no screenshots, so the Store keeps the ones it has.

## Consequences

- `./mosaik.sh --deploy --app` puts the listing online with the version. A part of the listing
  changed by hand in Partner Center is replaced by its file at the next send.
- `installer/msix/test-publish.ps1`, case 1d, against the stand-in of the Store
  (`store-standin.mjs`, which checks a listing it is given): the description, features, search
  terms and two captioned screenshots sent, the screenshot there before marked for removal, both
  new ones in the ZIP at its root, a fr-ca folder left out and said so; then 21 features, refused
  before anything is sent. Without a listing, the stand-in now also checks that the listing's
  pictures are left as they were. Run in the cloud under PowerShell 7.4 on Linux, the cases that
  keep a key with DPAPI left out there; on the owner's PC it runs with the rest of `mosaik.ps1 test`.

## Amended 2026-10-28: the short description

The launch kit's short description was still to be pasted in Partner Center by hand, the one part
of the English listing the files did not carry. `short-description.txt` now goes as the listing's
`shortDescription` (the submission API's field for the "Short description" shown at the top of the
listing), checked against the Store's limit of 1,000 characters before anything is sent; without
the file the Store keeps its own. Case 1d sends it and the stand-in checks it (kept as it was when
there is no listing); a short description of 1,001 characters is refused before anything is sent.
The whole of `test-publish.ps1` passes under PowerShell 7 on Linux.

## Amended 2026-10-28: the screenshots

The Store still showed the launch's screenshots, put up by hand: the window small and in the light
theme, on a wide grey background, hard to read at the Store's sizes. `listing/en-us/screenshots/`
now holds six, 2560×1440, in the order of their names, with `captions.txt`: a headline and a line
on Mosaïk's colours beside its window in the dark theme (Home, a workspace's page with its tabs,
its preview, the zone editor and Settings, from the interface's development server and its stand-in
engine), and a recording of a desktop arranged in numbered, coloured zones. The next send replaces
the Store's screenshots with them.
