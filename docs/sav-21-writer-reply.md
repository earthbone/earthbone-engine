# SAV-21 writer reply

The partial implementation already decoded the nine observed Stacked frames.
The missing review artifact is now present. The reader retains the named child
fields and four-byte branch index, and the corpus tests verify exact
re-encoding and the following extra-data boundary.

The required xEdit source names PackageData but does not define its named
payload. The decoder therefore follows the observed frame layout documented
in [saved package runtime state](save-package-data.md#stacked-boundary-correction-2026-10-02).
It does not infer a boundary by scanning for another extra or a terminator.

With `SOE_TEST_SAVES` set to the complete 60-save corpus, these checks pass:

- `sav_21_all_observed_stacked_payloads_roundtrip_and_keep_the_following_boundary`
- `sav_21_all_observed_stacked_payloads_reencode_and_preserve_the_following_boundary`
- `sav_21_all_observed_stacked_entries_reencode_and_keep_their_successors`

The first check rejects no observed Stacked payload. The latter checks confirm
all nine frames, exact bytes and decoded successor sections. Their synthetic
cases also cover truncated frames, an absent Sandbox request, the child index
and malformed bodies. This establishes compatibility with the observed
corpus, not a published general-purpose PackageData schema or runtime package
restoration.
