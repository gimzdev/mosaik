# 0033. A window dropped onto a zone joins it, a line dragged moves no window, and a workspace takes a layout with every app

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0031 (the picture, the first run's note), 0032 (drops onto a zone, stretched zones)

## Context

The owner, on 2026-10-16, after 1.0.2's pictures:

- "when dragging an app from a zone in the homescreen anim it permutes with the app of welcoming
  zone instead of just adding it (its correct in workspace edit screen the anim behaves correctly
  its just homescreen anim)".
- "same issue for stretch when resizing in homescreen anim it sometimes moves apps from a zone to
  another, the issue doesnt proc in workspace edit screen anim".
- "clicking quarters make most apps disappear", then "try coding for example theres 6 apps
  normally and going to quarter get rid of some" and "lets try workspace with most apps, on all
  layouts": a layout chosen while a workspace was shown on Home went back to the screen, which had
  fewer windows open than the workspace has apps.
- "in workspace edit screen the "add an open window" becomes "add an app" and proposes even apps
  that aint opened", then "why even specify "not open" if they open good if they not not they
  will so noone cares".
- "the apps in image 1 must be able to be dragged and droped from zones" (a workspace's list of
  apps, by zone).
- "windows theme doesnt work, either make it work or make it go we dont really care for it".
- Of the note the first run left above the workspaces ("Made from the windows you had open.
  Delete any you don’t need.", with Got it): "this shit has to go lets not flood with useless
  infos".

Measured before the change: on 20000 random screens with a layout picked, a line dragged moved a
window to another zone on 9918. The plan found the windows in the zones as stretched (a window
then filled a zone it had not, or suited a larger zone better), and an arranged screen sent a
window stacked in a zone made too small by the line to another zone.

## Decision

**A window dropped onto a zone itself joins it**, behind the windows there (`Placed::join`), or
takes it when it is empty, as before. No other window changes zone, and every zone keeps its
colour. When the zones cannot make room for it there without sending another window elsewhere,
or it fits none, the drop changes nothing. Dropped onto another window, the two still trade
places (ADR 0032), as on a workspace's picture. Hints: "Drag a window into a zone to add it
there, onto another window to swap them, or drag a line to resize zones."

**A zone keeps its colour through the user's moves** (job 0171: on the owner's PC, the half a
window left took the colour of the window then in front of it, as each zone took the colour its
own window wears). The colours of a picture with moves are those of the picture without them
(`Plan::unmoved`), as the moves left them (`NewZone::color`): only two zones whose own windows
trade places, or a zone and the empty zone its own window takes, trade colours (ADR 0032's tiles
in a mosaic); a window joining a zone or leaving it, or a stacked window brought in front,
changes none.

**A line dragged changes where the zones are, never which windows are in them.** The plan finds
the windows in the zones as the layout made them (or as they are on an arranged screen), then
the zones take the places drawn, made to hold every window in them, those stacked behind too
(step 4½, `stretch`, `fit::make_room`); places that cannot hold them are not taken. A window that
fit no zone may fit one once it is larger, behind the windows there. The picture before Mosaïk
answers, and the stand-in, do the same.

**A layout chosen while a workspace is shown lays out that workspace** (protocol 12:
`set_workspace_layout`, `undo_workspace_layout`, and `preset` on `preview_workspace` to see it
without saving), saved at once like the picture's other changes, with Undo while the workspace
has not changed since. Every app stays in a zone (`editing::with_layout`): each new zone takes
the old zone over most of it, with its id, colour and apps; the apps of an old zone left over go
to the new zone over most of it; a zone left empty takes the app at the back of the zone with the
most apps (a tab group moves as one). Automatic, which lays out the windows on screen, cannot be
chosen for a workspace. The layout the workspace is in is the one shown chosen.

**Add an app.** A workspace's page offers every app it could take in one list, by name, with a
box to find one: the windows open now that it does not have (two windows of one app told apart
by their titles) and the apps of the Start menu that are not open (`installed` in `apps`,
`add_installed_app`; `sets::startable` says how any entry is found and started; uninstallers,
folders and websites are no apps). An app not open goes in the zone asked, else the first empty
one, else the one with the fewest apps, behind them, and starts when the workspace opens.

**A workspace's apps are dragged in their list**: a row onto another app, the two trading places
as on the picture, or onto a zone, which it joins; a mouse drags from anywhere on the row, a
finger or pen from its grip; Esc lets go. Move stays the keyboard's way, and the picture's
Enter-to-pick.

**Like Windows follows Windows.** The title bar was made light or dark like the page, and Tauri
gives WebView2 the window's theme, so once a theme had been chosen the page never followed
Windows again. With Like Windows the window's theme is left to Windows (`style_title_bar`
without `dark`); only the caption takes the page's colours.

**The first run says nothing.** The workspaces it makes of the windows open are in Home's column
like any other, with no note to read or put away (it was the last such tip: `Tip` is gone).
Should the windows not be sorted, why waits in Mosaïk's notes, behind their button in the
window's bar (ADR 0024), with what to do instead: Save as workspace.

## Consequences

- Protocol 12. An older engine sends no `installed`, and the page offers its open windows.
- Core tests, each failing first on the code before: a drop onto a zone joins it behind its
  windows (Thirds, several drops) and only where it moves no other window; on windows wearing
  colours, joins and trades leave every zone its colour but where two own windows trade (on 3987
  random drops, 1164 changed a zone's colour before, none now); on 6000 random
  screens a line dragged moves no window out of its zone nor changes its place front to back; a
  line dragged on an arranged screen keeps the room a window stacked there needs; a layout picked
  keeps its windows' zones as the line moves; the owner's Coding on every layout, with and
  without a tab group, keeps every app in a zone with no zone empty while another holds two; any
  Start menu app can be added, uninstallers and folders cannot. ADR 0032's tests that dropped
  onto a zone to trade now drop onto the window.
- Interface tests: the first run makes its workspaces with nothing to read or put away, and a
  sorting that failed waits in the notes; the stand-in keeps every zone's colour as windows join
  others; the picture before Mosaïk answers for a drop onto a zone; Home lays out a
  workspace shown and undoes it; Like Windows leaves the window's theme to Windows; Add an app
  lists open and not open apps as one list, found by name, with the arrow keys; what each drop of
  the list asks. Playwright: a window dragged into another zone joins it and Move windows keeps
  it; the workspace with the most apps on every layout keeps every app, and Undo; apps dragged in
  their list; Add an app (with axe); the first run with no note.
- The `smart` probe drops a window onto the other half itself (it joins it, behind the window in
  front there, on the picture and on screen), and lays its workspace out on every layout (every
  app in a zone, nothing saved or moved), then in Quarters, and undoes it. The UI tour reads the
  page's background with Like Windows after Light and after Dark against Windows' app mode.
