# SPDX-License-Identifier: GPL-3.0-or-later
"""Author the TOOL-04 log without reading game data or calling the engine."""

import json
from pathlib import Path
import sys
import tomllib

fixture = Path(__file__).with_name("Papyrus.0.log")
probe = tomllib.loads((fixture.parents[2] / "probes/quest_wait.toml").read_text())
slots = {
    "Update.esm": 1,
    "Dawnguard.esm": 2,
    "HearthFires.esm": 3,
    "Dragonborn.esm": 4,
    "ccBGSSSE001-Fish.esm": 5,
    "ccBGSSSE025-AdvDSGS.esm": 6,
}
lights = {
    "ccQDRSSE001-SurvivalMode.esl": 0,
    "ccBGSSSE037-Curios.esl": 1,
    "_ResourcePack.esl": 2,
}
forms = {"runner": 0x07000800, "timescale": 0x3A}
for alias, form in probe["forms"].items():
    owner = form["plugin"]
    if owner in slots:
        word = (slots[owner] << 24) | form["object"]
    else:
        word = 0xFE000000 | (lights[owner] << 12) | form["object"]
    forms[alias] = word if word < 0x80000000 else word - 0x100000000

base = dict(
    schema=1,
    sequence=probe["name"],
    runtime_version="1.6.1170.0",
    skse_version=None,
    load_order=probe["load_order"],
    forms=forms,
    timescale=20.0,
    paused=0,
)
boundaries = [
    ("stage_10", "end", 0.0, {"accepted": 1, "stage": 10}),
    ("wait_1", "begin", 0.02, {}),
    ("wait_1", "end", 1.04, {"issued": 1}),
    ("stage_20", "end", 1.05, {"accepted": 1, "stage": 20}),
    ("wait_2", "begin", 1.06, {}),
    ("wait_2", "end", 2.08, {"issued": 1}),
    ("read_stage", "end", 2.09, {"stage": 20}),
]
lines = [
    "Papyrus synthetic fixture, authored for TOOL-04. No game session recorded.",
    "[10/02/2026 - 12:00:00PM] VM is thawing...",
]
for request, phase, elapsed, result in boundaries:
    observation = base | dict(
        request_id=request,
        phase=phase,
        real_seconds=elapsed,
        game_seconds=elapsed * 20,
        result=result,
    )
    lines.append(
        "[10/02/2026 - 12:00:00PM] SOEPROBE "
        + json.dumps(observation, separators=(",", ":"))
    )
generated = "\n".join(lines) + "\n"
if "--check" in sys.argv:
    if fixture.read_text() != generated:
        raise SystemExit("fixture differs from its authored generator")
else:
    fixture.write_text(generated)
