# SAV-70 writer reply

The reference-header reader already skips FORM, BASE and SCALE payload fields
for PCON type 42. The encoder now checks those presence bits only when the
header carries reference data, so the flags remain part of the opaque suffix.
PCON's typed prefix re-encodes byte for byte. PARW type 47 remains outside the
xEdit initial-data dispatch and retains its full payload through the change
envelope.

The local xEdit source `TES5Edit/Core/wbDefinitionsTES5Saves.pas` includes type
42 in `InitialDataTypeDecider`, omits type 47, and defines empty PCON and PARW
subtype structs. Those definitions support the current placement boundary and
opaque subtype handling.

With `SOE_TEST_SAVES` set, the SAV-70 corpus test checks every decoded reference
header's byte prefix and remainder boundary, and compares player reference
positions with the saved player location. Synthetic tests cover PCON flags,
opaque suffix preservation, PARW envelope preservation and malformed HAVOK
payloads. These checks cover the repository's 60-save corpus and the documented
xEdit layout. They do not restore opaque PCON or PARW subtype data into runtime
state.
