# Local files and saved progress

Open game uses Android's document picker. Select one CHD, PBP, ISO or PS-EXE,
or select one CUE together with **every BIN track it names**. Android's picker
supports multiple selection by holding a file and then selecting the rest.
A single document grant gives no access to neighboring files. The host
validates CUE companions and reports a named missing track before launch.
For now, CUE companions must be in the same directory with simple filenames;
paths into other folders are rejected. Matching names ignore case when
resolving the grant, then preserve the CUE's exact requested filename locally.

The host streams selected documents to private storage using a bounded
64 KB buffer; it does not load a disc into RAM. Imports require enough free
space for a private copy. They are committed only after the companions are
available. The recent-game entry points at that copy. Provider grants are
retained during copying and released after it, so continued play does not
depend on a cloud provider or removable storage staying connected.

ZIP and 7z archives must be extracted before selection. Standalone BINs,
M3U/multi-disc switching, SBI companion resolution, encrypted PBP files and
arbitrary subdirectory layouts are not supported by this initial host.
Formats the borrowed core advertises are not automatically qualified host
formats. Only the selected acceptance game will have device evidence.

The pinned donor contains an MIT-licensed OpenBIOS fallback from PCSX-Redux.
Its license is retained. This is a compatibility fallback, not a promise that
every game boots without original firmware. Select PS1 firmware from the
menu when a game needs a compatible dump; it stays private. The donor checks
its known firmware hashes and region. No proprietary firmware is bundled or
downloaded by the app.

Slot 1 uses the core's `Libretro` memory-card interface: a 128 KB card image.
The Rust owner restores it before the first frame, compares it each second,
and flushes on pause, focus loss, backgrounding, surface destruction and
normal closure. Updates use a same-directory temporary file, file sync,
rename and directory sync. A card with an unexpected length is preserved
and reported, never silently replaced. Slot 2 is disabled for this milestone.

Save identity is derived from the selected primary file and its companion
filenames/content hashes, independent of the document URI. Changing a game
dump or its filenames creates a distinct card. Reopening the same imported
copy preserves its card. Optional emulator save states and automatic state
resume are not implemented, so they cannot roll back newer card progress.
An uncatchable process kill during an unfinished save may lose recent
unsynced changes; acceptance requires completing an ordinary in-game save
and verifying it after a fresh app process.

App-private imported games, firmware, recent metadata and cards are local.
The application has no Internet or broad storage permission. Removing the
application deletes that private data; development updates must preserve it.
