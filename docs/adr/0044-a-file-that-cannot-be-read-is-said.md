# 0044. A file that cannot be read is said, once; its copy is one change behind

- Status: accepted
- Date: 2026-11-15

## Context

`settings.json` and `workspaces.json` are written atomically, and each had a copy beside it, made
when the file last loaded well, that is, when Mosaïk last started (`*.last-good.json`,
`mosaik_core::store`). The store's own rule is that a file that cannot be read is never
overwritten silently: it is kept, and the user is told. A pass over the app for 1.0.6 found that
the rule did not hold, and that the files could be let down in other ways:

- A file that could not be read, with a copy: Mosaïk started on the copy and said nothing
  (`Loaded::source` was read by no one). What was saved since the start before was gone without a
  word, and so was the reason. Mosaïk can run for weeks: the copy was as old as that.
- A file that could not be read, without a copy: the message held itself ("… is damaged and
  there is no good copy to fall back on: … is damaged and there is no good copy …") and named
  the file itself as the one kept, before the real name.
- A file that read well, whose copy could not be written (a full disk, the copy held by another
  program): the load failed. Mosaïk started on its default settings, or showed no workspace and
  saved none, beside files that were sound.
- A file saved by hand with a byte order mark (Windows PowerShell 5.1's `Set-Content -Encoding
  UTF8`, some editors): "not valid JSON", put aside as damaged, the edit gone.
- A workspace with a problem only a file edited by hand can have (an id used twice, an app whose
  zone is gone, a zone or an app on a monitor the workspace does not know) was skipped in part
  when it opened and told again at every start, for good: nothing in Mosaïk's window puts it
  right. Three apps under one id were told as the same sentence twice in a row, and a layout
  picked for such a workspace never ended (fixed apart, in `editing::with_layout`).
- The journal of hidden windows went through the same store, so it had a copy too: the list of
  what was hidden at another moment. With the journal missing, a rescue went by that copy.

## Decision

- Going back to the copy is said, in the notes the load already returns, so every caller has it:
  which file, why (the reading's own reason, in a few words), that the last change saved may not
  be in the copy, and the name the unreadable file is kept under (its name alone: it is in
  Mosaïk's own folder). A file missing while its copy is there is said the same way. The copy is
  then written back as the file, as a file moved forward or mended is: the next start reads it
  and says nothing. Said once.
- The copy follows the file: each save keeps the file it is about to replace, when that file
  reads well and is not the copy already (`Store::keep_copy`). So the copy is one change behind
  at most, not a session. A file that does not read well, damaged by hand since it was loaded, is
  never kept: the copy before it stays.
- The reason of a file that cannot be read is the reading's own, never a whole message; the
  message is made once, where the file's fate is known.
- The copy is a convenience: one that cannot be written does not stop the file from loading or
  from being saved, and a write that fails leaves no temporary file beside it.
- A byte order mark before the text is skipped, and the file written back without it.
- A workspace's problems are mended when the file is read, as its damaged tab groups already
  were (`Layout::repair_places`, after `repair_tabs`): a part whose id another has takes a new
  one, the first keeping it with what points at it; a zone or an app on a monitor the workspace
  does not know goes to its first (one is made up, standing for the main screen, when it knows
  none); an app whose zone is gone opens maximized. What was mended is said once, each kind of
  problem once with how often it came ("…, twice"), and the file is written back sound. Under
  "Your workspaces": the engine no longer says the workspaces file's notes under "Mosaïk's
  settings".
- The journal keeps no copy (`Saved::KEEPS_COPY`): a missing journal is an empty one, a damaged
  one is put aside and said, and either way a rescue goes by the marker each hidden window
  carries (ADR 0006). A copy of it left by an earlier version is removed.

## Consequences

- One more read of the file at each save of the settings or the workspaces, and one more write
  when the file changed since its copy was made. Workspaces are saved on their own thread
  (`mosaik_core::saver`); settings are a kilobyte or two, saved when the user changes one.
- The last change saved can still be missing after a fall back: the note says so.
- A mend is a choice made for the user: an app whose zone is gone could have been taken out of
  the workspace instead of opening maximized. It follows what was already done for the windows
  of a damaged tab group, and the note says what was done.
- The journal's notes are not shown, as before (a rescue reports what it restored).
- Tests: `mosaik_core::store` and `mosaik_core::layout` have one for each point, failing on the
  code before; `tests/any_file.rs` reads workspaces made at random and finds none with a problem
  left, or mended twice.
