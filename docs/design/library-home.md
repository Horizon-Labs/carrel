# Design: the Library, and a sidebar around what owners do

Status: accepted 2026-11-02, with the five decisions below. Issue #512.

## What this adds

Opening an archive greeted the owner with Status, a dashboard, and a sidebar that led with work queues (Review, Questions, Restructure, Active, Flagged). The owner's direction: the collection itself is the star. An archive opens on its contents, shown in a way that invites browsing; adding is a mode to switch on; and the queues draw attention only when there's something to decide.

## What owners do, most often first

1. Browse and rediscover what's there.
2. Find one thing (search).
3. Add something: a file, a web address, a note.
4. Decide what Carrel queued: review, questions, restructure proposals, flags.
5. Work with checked-out items.
6. Look after the archive: status, history, backups, sync.

The old sidebar was close to the reverse.

## How similar apps do it

- **Photos** on Mac and iPad opens on the Library itself. Its sidebar has pinned items, then albums, then a Utilities section at the bottom (Imports, Duplicates, Recently Deleted).
- **Music** opens on the library.
- **DEVONthink** keeps an Inbox for unfiled items at the top of the sidebar, beside the databases and smart groups.
- **Readwise Reader** separates what's kept (Library) from what waits to be dealt with (Inbox), and expects the Inbox to be cleared.
- **Mail** shows what needs attention as badges on rows, not as a screen of its own.
- In all of them, adding is a button or a drop, not a place to go.

## The sidebar

```text
Library              ← opens here
Checked Out      2   ← only when anything is checked out or on the desk

WAITING              ← only when there's something, with badges
  Review         5
  Questions      1
  Restructure    3   ← each row only when it has any
  Flagged        1

COLLECTIONS
  ▸ Reading     42
  ▸ Projects    17
    Holding      4

DISCOVER             ← collapsed until opened; remembers being opened
  On This Day · Wander · Timeline · Archive Map · Activity · Connections

ARCHIVE
  Status · History · Backups
```

**Waiting** is the section's name, not "Inbox": Carrel already uses "inbox" for each device's queue of changes for the writer. **Checked Out** is the screen called Active before; its links and `carrel tui active` keep the name `active`. A screen that's showing keeps its row while it's empty, so finishing the last proposal doesn't pull the row out from under the owner.

## The Library page

- **Collections as covers:** the top-level collections, each with a few of its newest items' thumbnails, its title, its description and how many items it holds, nested collections included. Holding comes last. Choosing one opens it, as in the sidebar.
- **Recently Added:** the newest items, by the day they were filed.
- **On This Day:** Discover's On This Day, when there's anything.
- **A banner when something waits,** such as "5 to review · 1 question". It opens the first of those screens, and goes when nothing waits.
- **An empty archive** invites adding: "Drop files here or choose + to add."

Restricted items never appear on a cover.

## Adding is a mode

- **The + button** in the toolbar, **File → Add Items…** (⌘N, which the window doesn't otherwise use: New Archive is ⇧⌘N), and dropping anywhere on the window add to the archive. On the Mac, + and ⌘N open the **Add panel**; dropping goes straight to the add sheet, as before.
- **The Add panel** holds what Status's intake section held: choosing files, a web address, a note or the clipboard, and the intake queue with the worker's progress.
- **Intake leaves Status,** which keeps one line saying what's being added.

## In each interface

- **The core** (`carrel_core::library`) computes the Library from the discovery catalog: the shelves with their counts and covers, Recently Added, and how many are flagged. It also decides which Waiting rows show (`Waiting::rows`) and words the banner (`Waiting::line`). The apps reach it through `ArchiveHandle.library` and `waitingRows` in the FFI.
- **The command line:** `carrel library` prints the same: the shelves with their counts and descriptions, Recently Added, On This Day and what waits, or one JSON value with `--json`.
- **The interactive mode** uses the same order in its row of tabs: **1 Library**, **2 Checked Out**, **3 Review**, **4 Questions**, **5 Browse**, **D Discover**, **6 Status**, **7 History**, **8 Backups**, **9 Settings**, **L Librarian**. Checked Out, Review and Questions are left out of the row while they're empty, as the sidebar leaves them out; their keys still work. It opens on Library: what waits, the shelves with counts, Recently Added and On This Day, each a row that `Enter` opens. Restructure proposals are in Review there and flags on Status, so their Waiting rows open those. Adding is a key, not a tab: `+` shows the intake queue, and `i` adds from anywhere, as before. `carrel tui library` starts on it.
- **The Mac app, iPad and iPhone** use the same sidebar; on iPhone, Library is the first screen. `carrel://open?screen=library` and `carrel gui library` open it. On iPad and iPhone the + menu also has Photos, Take Photo and Scan Document, and **Show Queue…** opens the Add panel as a sheet.

## Building it

1. The core's `library` module, and this design (#512).
2. `carrel library`, and the interactive mode's Library and order (#512).
3. The FFI's `library`, and the apps' Library page, sidebar and Add panel (#512).
4. `docs/design/mac-app.md`, the user guide and the Mac app's guide, which the wiki publishes (#512).

## Decisions (accepted 2026-11-02)

1. **Every archive opens on a Library of its contents,** in every interface. Recommended: browsing and rediscovering is what owners do most, and it's what Photos and Music open on.
2. **The section of queues is called Waiting,** shown only when something waits, with each row only when it has any, and badges. Recommended: Mail's badges draw attention without a screen of their own, and "Inbox" already means each device's queue of changes.
3. **Adding is a mode:** +, ⌘N and dropping open the Add panel, which takes Status's intake section; Status keeps one line. Recommended: adding is a button or a drop in every comparable app, not a place.
4. **Discover is collapsed until opened, and remembers being opened.** Recommended: its six views are for wandering, not for every visit, and the sidebar stays short.
5. **Status, History and Backups go under an Archive heading at the bottom.** Recommended: looking after the archive is what owners do least often, as Photos keeps Utilities at the bottom.
