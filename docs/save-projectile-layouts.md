# Saved projectile and hazard layout boundary

SAV-55 records a shared source prerequisite for SAV-11, SAV-23 and SAV-25.
Their original decoder completion checks remain unchanged. The sources below
do not establish exact serialized layouts for all eight subtypes, so those
decoders remain blocked. This audit adds no parser or runtime behavior.

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

## Required evidence

A public Skyrim serialization definition or controlled vanilla fixtures must
establish the subtype binding, supported versions, flag order and exact field
boundaries for each subtype. Fixtures need recorded build versions, identities,
operations and independently observed state. Parsing must account for the
complete payload and reject unsupported combinations before state changes.

The SAV-11 research checkpoint `4d2afa8` surveyed 60 saves and found 40 PMIS
payloads totaling 10,854 bytes, 169 PGRE payloads totaling 29,169 bytes and no
PBEA payloads. That is coverage evidence, not a decoded schema. PBEA and the
other subtypes need their own coverage accounting when a decoder is available.
