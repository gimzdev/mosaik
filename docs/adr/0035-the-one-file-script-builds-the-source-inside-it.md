# 0035. The one-file mosaik.ps1 builds the source inside it

- Status: accepted; amended by ADR 0040 (one folder, `mosaik`, whatever the version; a repository there worked in as it is)
- Date: 2026-10-16

## Context

The owner, on 2026-10-16 at 20:04, with a picture of Home as it was on 2026-10-15 (Arrange and
More layouts): "why i got old version i just rebuilt the app from the mosaik/mosaik.ps1". The
one-file `mosaik.ps1` beside his repository unpacks its source to `mosaik-<version>` beside it
and builds there. That folder was there already, unpacked the day before by an older
`mosaik.ps1` of the same version (1.0.2): it was taken as it was, and its source a day old was
built. The version stays 1.0.2 until the next release, so the version alone cannot say whether a
folder holds the source inside the script.

Job 0177 then ran the first fix as the owner does: the folder was brought to the new source, but
the files written carried the date the source inside gives every file (2026-01-01), older than
the last build. Cargo builds what changed by the dates of the files: it took them for built, and
the build failed. The Mosaïk that had quit for the build was not started again: none ran after.

## Decision

**An unpacked folder records which source it holds**: `.mosaik-source.json`, with the fingerprint
of the source inside the script that unpacked it and each file's. Whenever a `mosaik.ps1` works
on the folder (build, test, run, pack, unpack, the menu) and its own source is another, it first
brings the folder to its own:

- files changed, added and gone, as its source has them, written now and so dated now;
- the build (`target`, `node_modules`, `dist`) kept, so a build after it is an ordinary one;
- a file changed since it was unpacked moved to `.mosaik-replaced\<when>\` first, never lost,
  and said; a file of the person's own, which no source has, left where it is;
- a folder from before the record is brought up to date the same way, its files dated as the
  source inside dates them taken for the old source's, the others for the person's.

**A build that fails starts again the Mosaïk that quit for it**, as it was, and says so; the
failure is still passed on.

## Consequences

- `installer/test-mosaik-ps1-unpack.ps1`, run by `mosaik.ps1 test` (20 checks): a source packed
  twice under one version, unpacked, built, changed by the person, brought up to date, and how
  many files that was said to be; again with nothing to do; a folder with no record. `installer/test-mosaik-ps1.ps1` (14 cases): a build
  failing with a Mosaïk running, and with none.
- The owner's folder from job 0177, its files dated before its build, had them dated now once
  (job 0178), the one folder written by the first fix.
