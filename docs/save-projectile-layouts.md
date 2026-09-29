# Saved projectile and hazard layout boundary

SAV-55 records a shared source prerequisite for SAV-11, SAV-23 and SAV-25.
Their original decoder completion checks remain unchanged. The sources below
do not establish exact serialized layouts for all eight subtypes, so their
payload fields remain undecoded. A synthetic SAV-55 regression test pins the
eight public type indices and the generic change-form header's flag and
payload round trip. SAV-23 additionally exposes all shared xEdit PFLA, PHZD
and PBAR change flag bits as typed positions and retains the inflated payload
bytes. It does not assign payload fields or establish internal boundaries.

## Local source evidence

The inspected xEdit revision is `6dbbed51d92456e39ffb916eece56040e91114b5`.
Its `Core/wbDefinitionsTES5Saves.pas` has SHA-256
`229cc3df9a8daacf68f0f1da511acff02a96e3803ba158726aac1748a9b7fdec`.
`wbChangedProjectile` appears as a declaration and a structure assignment,
but no change-form dispatch branch uses it. The PMIS, PGRE, PBEA, PFLA,
PHZD, PBAR, PCON and PARW dispatch structures are empty.

The unused structure contains a common reference prefix, unnamed words,
reference links, an inventory discriminator and further fields. Its existence
does not establish which subtype uses it or whether those fields are complete.
`InitialDataTypeDecider` includes several projectile types but omits PARW,
while `ChangedFormExtraDecider` includes PARW. That difference needs evidence
before reusing the reference prefix for arrows.

xEdit does define the shared 32-bit change flag word for PFLA, PHZD and PBAR.
Those definitions reuse reference change names and retain unnamed positions.
The UESP `REFR_Changeform.wikitext` describes a REFR payload, but does not bind
that payload layout to projectile or hazard subtypes. The PFLA, PHZD and PBAR
payload branches in xEdit's changed-form union are empty, so the shared flag
word does not establish the bytes that follow it.

FallrimTools `ChangeForm.java` at revision
`61bb57d895f023617cb55045a4907849fd1ff566` supplies no typed reader for these
subtypes in `getData`. Its raw fallback preserves bytes without identifying
their fields or boundaries.

CommonLibSSE-NG `include/RE/H/Hazard.h` declares `Hazard` as a subclass of
`TESObjectREFR`, with its own save and load methods. It is not a projectile
subclass. `include/RE/A/ArrowProjectile.h` derives from `MissileProjectile`
and declares its own save and load overrides. Runtime inheritance and member
offsets alone do not define ESS serialization. In particular, neither applying
the unused projectile structure to hazards nor assuming PARW equals PMIS is
justified by these declarations.

The FO4 and earlier xEdit save definitions contain related projectile
structures, but they are for different games and do not establish Skyrim SE
field layouts. The Skyrim definitions remain the only relevant xEdit source
consulted here. FallrimTools' `ChangeForm.java` reads the payload as raw bytes
when no typed reader is registered. These references therefore do not supply
the missing field order or flag semantics.

## Required evidence

A public Skyrim serialization definition or controlled vanilla fixtures must
establish the subtype binding, supported versions, flag order and exact field
boundaries for each subtype. Fixtures need recorded build versions, identities,
operations and independently observed state. Parsing must account for the
complete payload and reject unsupported combinations before state changes.

The current decoder covers the shared flag word and exact payload retention,
not subtype fields. Its truncation check covers the generic length-delimited
change-form envelope. The SAV-23 survey of 60 local saves inspected 1,344,172
change forms and found 13,240 PHZD payloads totaling 525,606 inflated bytes.
It found no PFLA or PBAR payloads. The observed PHZD flags were
`00000001`, `10000000`, `80000002`, `80000006`, `82000002`, `82000003`,
`90000002`, `a000000a`, `a000000e` and `a200000a`, all at form and change
version 78. This is coverage evidence, not controlled operations or
independent per-field observations. A typed payload decoder still needs an
authoritative subtype layout or controlled fixtures before it can close this
prerequisite.

The SAV-11 research checkpoint `4d2afa8` surveyed 60 saves and found 40 PMIS
payloads totaling 10,854 bytes, 169 PGRE payloads totaling 29,169 bytes and no
PBEA payloads. That is coverage evidence, not a decoded schema. PBEA and the
other subtypes need their own coverage accounting when a decoder is available.
