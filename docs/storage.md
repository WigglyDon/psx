# Development storage

The workstation is shared. Btrfs can exhaust metadata allocation while `df`
still reports free bytes. Check capacity before large writes; retaining another
expanded copy of reproducible source is not a backup strategy.

`cargo xtask storage` reports project classes and current capacity. Allocated
file-block estimates can share extents or be compressed on Btrfs; do not add
them together or call them physical reclaim. Record actual filesystem and
allocation readings before and after cleanup.

```sh
cargo xtask storage
cargo xtask storage check 3
cargo xtask storage audit
cargo xtask storage prune
cargo xtask storage audit-backups
cargo xtask storage compact-backups
```

`check` takes the estimated additional GiB for the operation. A previously
built `target/release/xtask storage check 3` avoids compiling a tool during
pressure or device custody. `audit` verifies without deletion; `prune` removes
only verified expanded source stages, with durable private receipts. It keeps
unrecognized, incomplete or differing trees and explains why.

## Capacity and concurrency

The initial local guard reserves 12 GiB of available filesystem space plus
the operation's estimated growth: 8 GiB for an APK or standalone PS2 build,
3 GiB for corresponding source, 1 GiB for firmware reproduction and 64 MiB
for notices. On Btrfs it additionally requires 512 MiB of usable logical metadata
outside the unused global emergency reserve. When device-unallocated space is
below 1 GiB, retain 1 GiB of usable metadata instead. Already allocated unused
metadata can serve ordinary writes; low unallocated space alone does not mean
ENOSPC. Any emergency-reserve use or missing allocation/reserve readings refuses
growth. These conservative starting values are not a validated
host quota or a claim that simultaneous workloads fit; the host custodian
owns changes to shared capacity policy.

The metadata distinction follows the [Btrfs filesystem accounting](https://btrfs.readthedocs.io/en/latest/btrfs-filesystem.html):
the global reserve is part of metadata, while device-unallocated space is needed
to expand block groups. The project initially required an unallocated chunk
unconditionally; this rejected a host with ample allocated metadata even after
bounded data compaction. The revised guard keeps a larger existing metadata
reserve in that condition, without automatic privileged filesystem operations.

Writing xtask operations hold one descriptor-owned project lock. Capacity is
checked before large jobs and between subprocess stages; existing caches
remain reusable. This does not reserve bytes against other projects or bound
a compiler while it runs. Direct Cargo/tools, backups, media transfers and
experiments must run an explicit preflight with their actual expected peak.
If another job consumes the margin, stop new writes and report pressure.
No storage check performs balance, snapshots cleanup, package changes or
filesystem repair. Read-only verification and retention remain available.

An independently admitted removable build workspace may be selected with
`PSX_BUILD_WORKSPACE`, `PSX_BUILD_VOLUME_UUID`, `PSX_BUILD_VOLUME_MOUNT` and
`PSX_BUILD_CAPACITY_LEASE`. All four are required together. The build verifies
the exact mounted ext4 volume, private project root and existing external lease,
then holds that lease plus its project lock. `target`, the PS2 native cache and
`TMPDIR` must already resolve inside that root through separately authorized
cache placement. APK staging/output and repeated capacity checks use the actual
workspace; absent/changed media refuse without internal fallback. Existing
internal APK stages are not moved. This route permits only warm native builds,
retains locked dependency outputs, and requires `PSX_REUSE_PS1_SHA256` to match
the existing pinned PS1 native artifact instead of rebuilding it. Source archives
use their separate streaming admission. These settings are host build inputs,
not application options or storage/device admission authority.

## Source bundles

Completed `build/psx-source-<revision>.tar.gz` archives and their SHA-256
sidecars are canonical corresponding-source artifacts. Reuse an existing
verified bundle for the same revision. Successful source creation retires its
expanded stage after verification instead of accumulating one expanded tree
per commit. Old completed stages can be retired explicitly with `prune`.

A stage is eligible only directly under `build/source-bundles`, with its
original revision marker, a corresponding Git commit, a complete matching
archive/checksum and the same archived full revision. Compare the complete
path set, contents, permissions, types and symlink targets. Ignore only the
ownership/timestamp normalization used by release tarballs. Extra, changed,
redirected, unmarked or incomplete work is kept. Recheck identity before
deletion, record proof first, and retain the archive and receipts.

`compact-backups` explicitly retires redundant expansions of PLAYER-2 device
backups under `.local`, keeping each complete matching TAR and durable proof.
It compares the entire path set, file contents, modes and types, then rechecks
the directory and archive identity before removal. Only an app-files-only
expansion with an ordinary same-name TAR qualifies. Only the donor's exact
app-private PS2 firmware/resource links are allowed, compared without following
them; other links, extra notes/work, incomplete archives or differing data are
preserved. `audit-backups` performs
the same checks without removing the expansion. This compacts two copies of
the same backup; it never expires a backup archive, save, media snapshot or
unique evidence. Prefer reading selected archive members for later inspection.
Host ownership/timestamps and the enclosing `files` directory's extraction mode
may differ; the receipt records that mode and the TAR retains original metadata.
Nested modes, types, link targets and bytes must still match exactly.

An interrupted source build keeps its unfinished stage and failure evidence.
Do not retry by creating another checkout, duplicating dependency caches or
silently deleting that work. Inspect the failed operation and recover it
under the project's current assignment. Source checking should stream or
extract selected files rather than expand another complete bundle.

The narrow Linux host tool `source-update` implements the reviewed `09e27cd`
to `123e831` transformation using the installed libarchive 3 ABI. It verifies
the fixed predecessor digest, reads members sequentially, substitutes the four
immutable target files and two revision metadata files, and verifies a separate
streamed readback before publishing a complete archive. Cargo configuration,
native manifests and dependency source bodies stay in that archive; the update
does not need an extracted predecessor. Unsafe/duplicate names, unsupported
types, escaping links, truncated streams and failed writes refuse publication.
An incomplete output is retained instead of being treated as a canonical bundle.

Its `audit` and `retire` operations are restricted to the one recorded unfinished
target stage. They retain full content/type/mode/link/identity and failure
receipts first, compare proposed entries against the verified archive plus
immutable target objects, and preserve unknown or differing entries. A growth
refusal does not authorize a compiler or archive retry. Existing installed
comparison/inventory tools can bootstrap the explicitly authorized retention
when a new utility executable itself cannot be admitted; they must retain the
same independent proof, recheck and project-lock boundaries. Source repositories,
canonical/failed archives, binaries, symbols and private inputs are excluded.

The target revision remains `123e831` regardless of the later utility commit.
The publication receipt records that utility revision separately. This narrow
route does not change the normal source builder or qualify the shared host for
other workloads. See [build.md](build.md) for its invocation and validation gate.

The producer-repair successor uses `source-update producer-update <full-commit>`
against that exact complete `123e831` archive, not the older four-file delta.
It permits at most64 regular project-file additions/modifications, bounded to
16MiB combined, with unchanged dependency locks/configuration/exported trees.
It derives bodies/modes from immutable Git objects, supplies missing ordinary
parent directories, binds the seven native patches in release metadata, and
independently streams the complete output through the existing project/native/
Cargo validation. Every unchanged member is compared with the verified input.
Executable source, target revision and utility revision are recorded separately.
Collisions, type/mode changes, missing parents or unsafe/extra entries refuse;
incomplete output and its receipt are retained. No dependency tree is expanded.

## Protected work and other artifacts

The Vulkan player source route is `source-update vulkan-update <full-commit>`.
It streams from the frozen complete `d11d03e6d02c` archive, with the same bounded
project delta and independent complete readback. Its sole registry exception is
ash0.37.3+1.3.251: the lockfile must differ only by that exact package and its
player dependency edge. The crate digest must match the pin; safe regular source
members, both licenses and a complete Cargo checksum inventory are added directly
to the archive in memory. Other dependency bodies stay unchanged. No expanded
vendor tree, registry refresh or native rebuild is needed. Target/application,
native compilation and archive-utility revisions remain separate bindings.

`source-update repair-overlap-metadata` is restricted to the frozen complete
`47a703c` source archive and its recorded digest. Its patch file is present,
but an unanchored Git lookup omitted `0011` from generated patch metadata.
The corrected lookup uses the source repository and propagates Git failures.
The repair streams a distinct `-metadata-v2` archive, changes only
`SOURCE-CHECK.json`, and independently verifies every unchanged member and the
complete source graph. The original archive, application/native binaries and
symbols remain intact. Its target stays `47a703c`; the repair utility revision
is recorded separately. No engine/application rebuild or expanded tree is needed.

Under the installed finite host-workspace route, build/source utilities adopt
the custodian's actual inherited capacity descriptor after inode/token and lock
validation. They retain private-root, volume and reserve checks and the existing
project lock; opening a second competing capacity description would refuse the
custodian's own already-held phase. An environment label alone is insufficient.

Never automatically expire games, proprietary firmware, memory cards, saves,
checkpoints, controller profiles, device backups, owner-selected screenshots,
licensed fixtures, signing material, credentials, Git worktrees/unpushed work,
unknown files or host-recovery evidence. `.local` is not a disposable cache.
Keep the accepted PS1 baseline, installed candidate, current reviewed candidate,
required fallback and their exact source/signature/provenance evidence.
Existing qualified archives/candidates remain retained in this audit.

Old standalone source-validation Cargo targets may be retired after their
source archives and retained result/failure logs have been identified and
verified, with no active compiler/build job. Record the exact cache paths and
retention proof under the same storage lock; retain enclosing source, private
inputs, generated signing identity, qualified APKs and unique evidence. Use the
main target cache for new checks instead of accumulating one per source check.

Keep active Rust/native/dependency caches for current engineering. A future
cache retirement needs its own ownership, inactive-job and reconstruction
check; no global `cargo clean`, `git clean` or shared SDK/cache purge.
Shared SDK/NDK and host-tool caches outside this project belong to their
custodians and are excluded from project cleanup.

New device windows back up current app data without recopying game media.
Read selected backup members directly where possible; avoid another full
extraction merely to inspect one profile or checkpoint. Keep captures and
diagnostic runs focused and bounded to the actual question. Preserve current
failure/qualification logs and use one named output per check, not repeated
unindexed dumps. End each assignment with artifact identities and an explicit
retention decision; do not silently discard unfinished evidence to meet a
space target.

Private receipts live under `.local/storage-retention`. GitHub receives only
appropriate aggregate results and source/build identities, subject to the
existing device-evidence disclosure rules. Neither `du` nor a project's net
`df` change proves that concurrent host activity was caused by this project.
