# 0040. One folder for the app

- Status: accepted
- Date: 2026-10-17
- Amends: ADR 0035 (the one-file `mosaik.ps1` unpacked to `mosaik-<version>`)

## Context

The owner, on 2026-10-17 at 02:08: "why does mosaik/mosaik matter if thats basically the fucking
site ?", then "i think we should get one folder for site one for app why a third". His `gimz\mosaik`
held three folders: `mosaik`, the repository (the app's source and docs, the copy of the site that
CI checks, the history GitHub gets); `mosaik_site`, the site as `mosaik.sh` builds and deploys it;
and `mosaik-1.0.2`, a second copy of the app's source that the one-file `mosaik.ps1` beside the
repository had unpacked, where Mosaïk was built and ran. Version 1.0.3 would have unpacked a fourth.

## Decision

**The one-file `mosaik.ps1` works in one folder, `mosaik` beside it, whatever its version:** a
newer copy brings that folder up to date (ADR 0035), and no folder is made per version. **When
that folder is Mosaïk's Git repository, as on the owner's PC, the copy works in the repository as
it is**: it builds, tests, runs, packages and publishes there, never writes into it (a copy's files
would differ from git's, line endings included), and says whether the repository's source is the
one inside it. `mosaik.sh --app` already sent the app from the repository.

So the owner's `gimz\mosaik` holds the app (`mosaik`, built and run there) and the site
(`mosaik_site`), with the two files beside them: `mosaik.sh` and `mosaik.ps1`.

## Consequences

- Test, failing first (`installer/test-mosaik-ps1-unpack.ps1`, 31 checks): one folder whatever
  the version; a repository beside a copy is worked in as it is, its files as they were, no record
  of a source, nothing moved aside or unpacked into it, no other folder made, and the copy says
  whether its source is the repository's.
- On the owner's PC Mosaïk is built in the repository; `mosaik-1.0.2` is left from before, for the
  owner to delete.
