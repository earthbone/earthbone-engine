# soe-bsa: archives and the data view

`crates/bsa` reads Bethesda archives and answers "what bytes does the game see
at `meshes/x.nif`" the way the shipped engine does. It is a port of
linux-mod-organizer's `src/bsa/parser.rs` (Dr. Baklava, GPL-3.0-or-later),
kept under the same licence, with three additions: the TES4 name hash from
LMO's BSA writer, an in-memory read API, and Windows-1252 name decoding.
The BA2 record and DX10 chunk layouts were checked against the local BSArch
FO4 implementation in `engine-refs/bsa/src/bsa/fo4.cpp` and its FO4 archive
tests. Its DX10 layout agrees with the 24-byte record and chunk headers read
here, including the chunk-header size and trailing sentinel fields.

## Formats

| Format | Where | Compression |
|---|---|---|
| BSA v103 | Oblivion | zlib |
| BSA v104 | Fallout 3, New Vegas, Skyrim LE | zlib |
| BSA v105 | Skyrim SE and AE | LZ4 frame |
| BA2 GNRL | Fallout 4, Starfield general files | zlib per file |
| BA2 DX10 | Fallout 4, Starfield textures | zlib per chunk, DDS header rebuilt on read |

A compressed BSA entry starts with its uncompressed size as a u32 and then the
packed stream. Per-file compression is the archive default XOR the record's
bit 30. Names in every BSA version are decoded as Windows-1252, which is
lossless for ASCII and right for the accented names Skyrim SE mods ship.
Archives with flag 0x100 (every Skyrim SE texture archive) embed each file's
path at the front of its data block, before the size word. The record keeps
the block as stored and the read peels the prefix off, so an off-disk index
never has to look into the payload for it. BA2 names are read from the name
table at the end of the file.

## Two ways in

`index_archive(path)` reads only the header and tables. A 246 MB archive
describes 468 files in 20 KB, and a mod manager lists hundreds of archives per
refresh, so this path never touches the payload. A test counts bytes pulled
through the reader to keep it that way. `uncompressed_size` stays 0 for
compressed BSA entries on this path, because that number lives out in the
payload.

`Archive::open(path)` retains only the header and index tables, then reads the
selected record or DX10 chunk from its open file handle. A mutex covers the
seek and read operation, then decompression happens after that lock is
released, so shared archive reads cannot race while unrelated decoding can run
at the same time. This is the engine path and keeps archive payloads out of
startup memory. `Archive::from_bytes(path, data)` retains the in-memory API for
callers that already own archive bytes. `read_record(data, format, record)` is
the corresponding byte-slice decoder.

Every record read is bounds-checked against the archive length and reports
`Error::Truncated` with the name and offset instead of panicking, which is what
a cut-off download produces. Header counts larger than the file can hold are
refused before any allocation.

## The data view

`Vfs` applies the game's override rule:

1. loose files beat every archive,
2. among loose roots, the one added last wins,
3. among archives, the one added last wins.

The caller adds archives in the order the load order produces them, which is
`sResourceArchiveList` and `sResourceArchiveList2` from the INIs followed by
each plugin's associated archives in plugin order (`find_associated_archives`
implements the stem rule: `MyMod.esp` loads `MyMod.bsa` and `MyMod - X.bsa`).
Loose roots are added in the order the mod manager stacks them. `Vfs` never
reads the plugin list itself, that decision belongs to the load-order code.

Lookups go through `normalize`: lowercase, forward slashes, no leading
separator, no `data/` prefix. Every other crate that keys on a data path
should call the same function.

## Hash

`tes4_hash`, `file_name_hash` and `folder_hash` compute the value stored next
to every BSA name. The BSA reader checks each file hash against its filename
and, when directory names are stored, checks each directory hash against its
path. A mismatch returns a format error naming the path and stored and
calculated values. Archives without directory names still have their file
hashes checked. Hashing follows Wabbajack's `GetBSAHash` behavior and the
independent `bsa` reference vectors, including the quirk where the middle
loop stops two bytes short of the end of the name. Names are encoded as
Windows-1252 bytes for hashing, including accented Skyrim SE names.

## Writing

`write_bsa` builds a compressed BSA in memory for versions 103, 104 and 105.
It writes the uncompressed byte count before each compressed payload, using
zlib for versions 103 and 104 and an LZ4 frame for version 105. Archive entries
are grouped by folder hash and files are ordered by file hash. Lowercase path
and original path spellings break hash ties so input order does not affect the
archive bytes. File and folder hashes use the same TES4 hash verified by the
reader.

The writer includes folder and file names and stores names as Windows-1252.
Paths may use either slash direction, but absolute paths, empty components,
`.` and `..` components, duplicate paths ignoring case, and names outside
Windows-1252 are rejected. The implementation follows the table and payload
layout in Wabbajack's `Wabbajack.Compression.BSA/TES5Archive/Builder.cs` and
the compressed record framing read by the local Linux Mod Organizer BSA
parser. It writes each file compressed and does not embed paths in payloads.

## Tests

Synthetic archives cover every format and both compression paths, valid and
invalid BSA path hashes, a name
with a non-ASCII byte, a truncated payload, absurd header counts, a DX10
texture rebuilt into a DDS file, concurrent disk reads, a sparse 64 MiB
payload that stays off heap at open time, and the override rules of the view.
The structure-aware `fuzz-bsa-grammar` target builds BSA, BA2 GNRL and BA2
DX10 archives. It reads BA2 records through the archive API and checks that
DX10 reads rebuild the DDS header and recover each chunk's bytes.

`tests/real_data.rs` runs when `SKYRIM_DATA` points at a Skyrim SE `Data/`
folder: it indexes every vanilla archive, reads a record from each, and pulls
a known mesh, texture, SWF and PEX through the view by their plugin spellings.
On 2026-09-06 against the Steam install it indexed all 30 archives without a
failure, 172,961 files, and read the first record of each, which caught the
embedded-name layout of the texture archives that LMO's parser never handled. Not covered by any real file yet: v103, v104 and BA2 archives, since
only Skyrim SE is installed here.

## Live run (2026-09-14)

Reran the gated test against the same install. It indexed and read every file of
every archive, 172961 files across 23 archives, the eight base Skyrim archives, the
nine texture archives, the Creation Club archives and the resource pack, plus the
loose marketplace archive. The VFS test answered every vanilla path case
insensitively. Both tests passed in 12.2 seconds with no failures and no code
change was needed.
