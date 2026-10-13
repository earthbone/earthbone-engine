// SPDX-License-Identifier: GPL-3.0-or-later

//! Greeting choices follow SkyUI's local Vanilla/DialogueMenu.as topic list.
//! Synthetic DIAL, INFO and QUST fields follow local xEdit and UESP layouts.

use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin, Record, Tag};

fn info(id: u32, speaker: u32, flags: u16, text: &str) -> Record {
    let mut response = [0; 24];
    response[12] = 1;
    Record::new(Tag(*b"INFO"), id)
        .sub(Tag(*b"ENAM"), [flags.to_le_bytes(), [0; 2]].concat())
        .sub(Tag(*b"ANAM"), speaker.to_le_bytes())
        .sub(Tag(*b"TRDT"), response)
        .zsub(Tag(*b"NAM1"), text)
}

fn fixture() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    for (base, name) in [(7, "Player"), (0x500, "Speaker"), (0x501, "GoodbyeSpeaker")] {
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), base)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"ACBS"), [0; 24]),
        );
    }
    for (reference, base) in [(0x100, 0x500_u32), (0x101, 0x501)] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), reference)
                .sub(Tag(*b"NAME"), base.to_le_bytes())
                .sub(Tag(*b"DATA"), [0; 24]),
        );
    }
    plugin.push_top(
        Record::new(Tag(*b"QUST"), 0x800)
            .sub(Tag(*b"DNAM"), [1, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0])
            .sub(Tag(*b"NEXT"), []),
    );
    plugin.push_top(
        Record::new(Tag(*b"DLBR"), 0x1900)
            .sub(Tag(*b"QNAM"), 0x800_u32.to_le_bytes())
            .sub(Tag(*b"TNAM"), 0_u32.to_le_bytes())
            .sub(Tag(*b"DNAM"), 1_u32.to_le_bytes())
            .sub(Tag(*b"SNAM"), 0x900_u32.to_le_bytes()),
    );
    let mut topics = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    for (id, category, subtype, text, responses) in [
        (
            0x900,
            0,
            *b"CUST",
            "Ask about the road",
            vec![info(0xA10, 0x500, 0, "The road leads north.")],
        ),
        (
            0x901,
            7,
            *b"HELO",
            "",
            vec![
                info(0xA00, 0x500, 0, "Welcome."),
                info(0xA01, 0x501, 1, "Goodbye."),
            ],
        ),
    ] {
        let mut topic = Record::new(Tag(*b"DIAL"), id)
            .sub(Tag(*b"QNAM"), 0x800_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0, category, 0, 0])
            .sub(Tag(*b"SNAM"), subtype);
        if !text.is_empty() {
            topic = topic
                .zsub(Tag(*b"FULL"), text)
                .sub(Tag(*b"BNAM"), 0x1900_u32.to_le_bytes());
        }
        topics
            .entries
            .push(Entry::Record(plugin.push_record(topic)));
        let mut children = Group::new(GroupKind::TopicChildren(id));
        for response in responses {
            children
                .entries
                .push(Entry::Record(plugin.push_record(response)));
        }
        topics.entries.push(Entry::Group(children));
    }
    plugin.groups.push(topics);
    plugin
}

#[test]
fn hst_28_greeting_default_action_selects_topic_and_empty_goodbye_can_continue(
) -> std::io::Result<()> {
    let Some(binary) = std::env::var_os("SOE_GODOT_BIN") else {
        return Ok(());
    };
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("Ui05Config.esm"),
        fixture().to_bytes(),
    )?;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(binary)
        .args(["--headless", "--path"])
        .arg(root.join("godot"))
        .arg("--script")
        .arg(root.join("crates/godot-host/tests/hst_28_greeting_controls.gd"))
        .env("SOE_HST_28_FIXTURE", directory.path())
        .env("SOE_GODOT_TIMEOUT", "60")
        .env_remove("SKYRIM_DATA")
        .output()?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(output.status.success(), "Godot exit {}", output.status);
    assert!(!log.contains("SCRIPT ERROR:"));
    assert!(log.contains("PASS hst_28 greeting controls"));
    Ok(())
}
