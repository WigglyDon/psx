# Local files and saved progress

**Add games → Scan storage** checks accessible shared/removable volumes, then
lets you select games for a persistent local collection. It runs only when
requested. Android's all-files permission is granted in its system screen;
protected app data and system areas are not scanned. Results count unreadable
folders, rejected images and archives; a safety limit is reported if reached.
No filenames or file contents are uploaded.

CUE tracks must exist alongside the CUE. ISO/raw BIN candidates need a valid
optical volume descriptor; bounded boot metadata identifies PS1 or PS2.
PS-EXE and PS1 PBP signatures are checked. CHD headers identify containers,
with a console choice when ambiguous. These checks do not prove a disc is
undamaged or compatible with
the donor. Track BINs named by a CUE are not shown as duplicate games.
ZIP/7z archives require extraction before scanning or import.

Scan-selected discs remain at their original paths. Moving/deleting them,
revoking storage access or removing a volume may require restoring access or
rescanning. Only selected discs are streamed for content hashes; discovery
reads bounded headers and CUE text. No whole disc is loaded into RAM. The
local manifest remembers selections across process restarts.

Library thumbnails use an existing memory-card save icon when available,
including its small animation. Otherwise a distinct colored disc/initial
placeholder identifies the entry. These are local thumbnails, not downloaded
box covers; no artwork database or network lookup is involved.

**Browse files** is optional and uses Android's document picker. Select one
CHD, PBP, ISO or PS-EXE, or one CUE together with **every BIN track it names**.
Android's picker supports multiple selection by holding a file, then selecting
the rest. A document grant gives no access to neighboring files. Missing
CUE companions are reported before launch. CUE companions currently require
simple filenames in the same directory; subdirectory paths are rejected.
Imported track names preserve the CUE's case-sensitive spelling.

The document route streams private copies through a bounded 64 KB buffer.
It needs enough free space and commits only after companions are available.
Provider grants are retained during copying and released afterward; this
route survives provider revocation or removal of the source volume.

The scanner groups common `(Disc N)` / `(Disk N)` filename conventions while
retaining region/revision labels. This is a filename hint, not a game database.
Starting disc selection and in-session **Change disc** use a private generated
M3U and the donor's libretro disk callbacks. Swap when the game requests it.
All discs in an entry retain one memory card; adding a disc does not rename
that card. Swapping preserves the emulated machine. Close/reopen a running
game to incorporate newly added discs. Internal PBP discs, when exposed by
the donor, are available through Change disc.

External M3U import, SBI companion resolution, encrypted PBP files and
arbitrary CUE subdirectory layouts are not implemented. Donor-advertised
formats are not automatically qualified host formats.

The free fallback uses pinned MIT-licensed PCSX-Redux OpenBIOS built with its
documented fast-boot mode, skipping its shell/intro. Its exact source and license
are retained in `vendor/openbios`; Rust prepares it only if `system/openbios.bin`
is absent. Existing supplied firmware is preserved and configured regional
firmware keeps the donor's precedence. This does not promise every game boots without
original firmware. Select PS1 firmware from Settings when a game needs a
compatible dump; it stays private. The donor checks known hashes and region.
No proprietary firmware is bundled or downloaded.

PS1 slot 1 uses the core's Libretro interface: a 128 KB card, already inserted.
Use the game's own save menu or save points for ordinary completed saves.
Automatic emulator resume is separate from these memory-card saves.

Rust restores the card before the first frame, compares it each second and
flushes on pause, focus loss, background, surface destruction and normal
closure. Updates use a same-directory temporary file, file sync, rename and
directory sync. An unexpected card length is preserved and reported, never
silently replaced. PS1 slot 2 is disabled for this milestone.

Initial identity uses the primary file and companion names/content hashes,
independent of document URI. A new dump or renamed files can create another
identity. Reopening the same imported copy preserves its card. A disc set
keeps its existing card identity when more discs are merged, including discs
added before the original starting disc.
An uncatchable process kill during an unfinished save may lose unsynced
changes. Acceptance requires an ordinary save loaded in a fresh process.

Automatic sessions retain the emulated machine, selected disc and pause intent.
Backgrounding, explicit pause and Close game checkpoint the current session;
periodic checkpoints run every 30 seconds. Reopening the app resumes its active
game; choosing a closed game resumes its own checkpoint. Restart game boots
the disc again while retaining its memory card. A sudden kill can lose the
time since the last completed checkpoint.

Resume files are bounded, private and atomically replaced. Their header pins
the core revision and disc identities and records hashes of the machine
payload and contemporaneous card. A changed card or incompatible/damaged
checkpoint is reported before restoration; Restart game remains available.
The durable card is restored after the machine state, so an old checkpoint
cannot silently roll back newer card progress. Adding/reordering discs or
changing the selected starting disc may require restarting instead of resuming.

Imported games, firmware, collection metadata, controller layouts and cards
remain local. There is no Internet permission. The default development APK
requests shared-storage scan access. A scoped build omits that permission
and retains document import; see [build.md](build.md) and the unresolved
Play permission gate in [distribution.md](distribution.md). Uninstalling the
app deletes its private data; development updates must preserve it.

For eventual phone replacement, plan a user-selected local export/import
bundle containing stable entry IDs, ordinary cards, layouts and a manifest
with hashes and console/core revisions. Games and firmware stay separate.
Import must validate the bundle, stop the affected runtime and retain existing
progress until the owner explicitly resolves conflicts. Incompatible states
remain optional retained data; ordinary cards remain authoritative. This UI
is future work, not a current export capability or a cloud requirement.

## PLAYER-2 additions under validation

ISO/raw-sector images and CUE data tracks now inspect bounded ISO9660
`SYSTEM.CNF` metadata: `BOOT` identifies PS1 and `BOOT2` PS2. A shared
`PLAYSTATION` volume marker or filename does not identify the console. CHD
header versions 3–5 identify a container only; an ambiguous new entry offers
a compact PS1/PS2 choice. This does not establish image integrity or title
compatibility. The first PS2 acceptance input is an owner-selected ISO.
Other donor-advertised compressed/executable formats are not claimed.

A missing selected disc offers **Locate original disc** for that same entry.
The document worker verifies all contents against its original stored disc
identity before relinking, including when the primary filename changes.
Companion renaming that changes CUE contents is not silently accepted.
The entry ID, cards, checkpoint and remembered disc stay attached. An unknown
restored shortcut never resolves by a similar title. Collection taps and
shortcuts open the remembered disc directly; runtime PS1 disc replacement
remains available. PS2 runtime disc replacement is not yet supported.

The qualified PS2 route currently uses compatible user-supplied 4 MiB firmware;
its separate Settings import remains private. ROMDIR/ROMVER discovery selects
an explicit image for each entry, with its filename and SHA-256 retained in
`ps2/<GameId>/firmware.json`. Adding another image cannot change that binding;
changed or missing bytes refuse a new runtime and retain cards/checkpoints.
An older unbound PS2 checkpoint requires explicit restart with current cards;
multiple possible legacy firmware inputs refuse an automatic choice. This is
identity protection, not a compatibility claim from a header. A BIOS-free
default remains under evaluation. Each entry has its own donor-owned regular
card files under `ps2/<GameId>/pcsx2/memcards`, with shared pinned resources
and private firmware. The donor creates its own card format. Rust pauses its
native owner, requests a checked flush, syncs files and binds checkpoint
metadata to their current digest. It never replaces cards during native
ownership. Folder-card storage is unsupported by this host.

PS2 checkpoints allow the donor's 68 MiB bound and pin both its exact source
revision, adapter-patch hash and selected firmware hash. Orderly shutdown seals against the final
card files while retaining the writer lock. A changed card, failed seal or
incompatible state retains current card data and the old checkpoint, with an
explicit restart route. PS1 retains its existing 64 MiB state bound and card
restore behavior. Verified relinking compares stable disc identities rather
than old filesystem paths. Ordinary PS2 saves and automatic resume still
require separate device acceptance observations.

## Phone buttons

While a game is open, **Settings → Controller settings → Phone buttons** can
assign Volume Up and Volume Down to digital controller inputs for that game.
Holding a key holds its assigned input; releasing it lets go. Both start
unassigned. Menus and unassigned keys retain system volume, and the in-game
speaker control remains available when both physical keys are assigned.
The power button is not offered. PS2 also offers L3/R3; these bindings do not
simulate analog axes. Full external-controller support remains separate work.

Assignments are atomic local preferences in `phone-buttons.json`, keyed by the
existing game ID. They survive orientation changes, title edits and relinking;
resetting a display layout does not reset them. **Reset phone buttons** restores
system volume for this game without altering touch layouts, cards or sessions.
The first implementation has host/Android build validation; physical key
delivery, repeat behavior and system-volume fallback require a device check.
