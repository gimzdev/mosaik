# 0039. Scripts named after their app

- Status: accepted
- Date: 2026-10-17
- Amends: ADRs 0001, 0025 and 0026 (which name the site script `mosaik1.sh`)

## Context

The owner, on 2026-10-17 at 01:55: "why the fuck is metaforge called metaforge2 ? lets just
rename all the bash and ps1 with just the name of the site / app no 1 or 2 or whatever after".
The site script was `mosaik1.sh`, the name the brief gave it, beside `mosaik.ps1`; in the owner's
`gimz` folder the other apps' scripts were `dropdate1.sh`, `metaforge2.sh`, `grimoire1.sh` and
`gimzdev1.sh` (which called itself `v1.sh`). Their versions had already gone back to 1.0.0.

## Decision

**Each script is named after its app or site, with no number or version:** `mosaik.sh` (the
site, at the root of this repository and in the owner's copy beside it) and `mosaik.ps1` (the
app); `metaforge.sh`, `dropdate.sh`, `grimoire.sh` and `gimzdev.sh`. What the scripts call
themselves follows: their help, the copy made when a file has Windows line endings, and the marker
an install carries (`.metaforge` and `.grimoire`, like `.dropdate` and `.gimzdev`; an install
with `.metaforge-v2` or `.grimoire-v1` is recognised and its marker renamed). Data kept in
browsers keeps its key (MetaForge's saved builds), so nobody loses theirs.

## Consequences

- CI, the README, `docs/RELEASE.md`, `docs/architecture.md` and `mosaik.ps1` (which leaves the
  site script out of the source it carries) name `mosaik.sh`; `site/` is written again and says
  so. ADRs 0001, 0025 and 0026 keep the name they were written with, with a note.
- The owner's files were renamed in place, his own settings in them (the Store's key line in his
  `mosaik.sh`, the mail key in `gimzdev.sh`) kept as they were. The Project keeps
  `claude/mosaik.sh` beside `claude/mosaik.ps1`, in place of `mosaik1.sh`.
