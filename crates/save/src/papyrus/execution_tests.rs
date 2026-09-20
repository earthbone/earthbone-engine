// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn words(out: &mut Vec<u8>, values: &[u32]) {
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn message(out: &mut Vec<u8>) {
    out.push(7);
    words(out, &[1, 2]);
    out.push(1);
    words(out, &[1]);
    out.extend_from_slice(&0x1_0000_0000u64.to_le_bytes());
    words(out, &[1]);
    out.push(3);
    words(out, &[42]);
}

fn fixture() -> Vec<u8> {
    let mut out = Vec::new();
    words(&mut out, &[2]);
    out.extend_from_slice(&[1, 2, 0]);
    words(&mut out, &[0xdead_beef]);
    out.extend_from_slice(&[1, 99]);
    words(&mut out, &[123]);
    out.push(3);
    words(&mut out, &[10]);
    out.extend_from_slice(b"QuestStage");
    out.extend_from_slice(&[0x40, 0, 7, 200, 0, 5, 3]);
    words(&mut out, &[456, 1, 2]);
    out.extend_from_slice(&[0, 0]);
    words(&mut out, &[1, 0, 2, 3]);
    out.extend_from_slice(&[3, 2]);
    words(&mut out, &[4, 0, 987]);
    out.push(0);
    out.extend_from_slice(&1u16.to_le_bytes());
    words(&mut out, &[2, 4]);
    out.extend_from_slice(&1u16.to_le_bytes());
    words(&mut out, &[3, 4]);
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&[23, 1]);
    words(&mut out, &[2]);
    out.push(1);
    words(&mut out, &[1]);
    out.extend_from_slice(&[0, 3]);
    words(&mut out, &[2]);
    out.push(4);
    words(&mut out, &[0x7fc0_0123]);
    out.extend_from_slice(&[5, 255, 26, 8]);
    words(&mut out, &[3, 1]);
    out.push(1);
    words(&mut out, &[1]);
    out.extend_from_slice(&0x1_0000_0000u64.to_le_bytes());
    out.push(3);
    words(&mut out, &[17]);
    out.push(2);
    words(&mut out, &[4]);
    out.push(33);
    words(&mut out, &[1]);
    out.extend_from_slice(&[0, 0, 0]);
    words(&mut out, &[0]);
    out.extend_from_slice(&[0, 0, 2, 5]);
    words(&mut out, &[u32::MAX, 0, 3]);
    out.push(1);
    words(&mut out, &[2]);
    out.push(1);
    message(&mut out);
    out.extend_from_slice(&[3, 0, 0x80]);
    words(&mut out, &[999]);
    out.push(0);
    words(&mut out, &[1, 2]);
    out.push(1);
    message(&mut out);
    words(&mut out, &[1, 2]);
    out.push(0);
    words(&mut out, &[1, 55, 1]);
    out.extend_from_slice(&u64::MAX.to_le_bytes());
    words(&mut out, &[1]);
    out.extend_from_slice(&0x1_0000_0000u64.to_le_bytes());
    words(&mut out, &[0x8765_4321]);
    out.extend_from_slice(&9u16.to_le_bytes());
    out
}

fn read(bytes: &[u8]) -> Result<Execution> {
    Execution::read(&mut Cursor::new(bytes, "test"), 5, BTreeSet::from([1, 2]))
}

#[test]
fn execution_keeps_frame_state_messages_and_file_order() {
    let bytes = fixture();
    let data = read(&bytes).unwrap();
    assert_eq!(data.active_data[0].id, 2);
    let frame = &data.active_data[0].frames[0];
    assert_eq!(frame.pointer, 1);
    assert_eq!(
        frame.variables,
        [Value::Integer(17), Value::String(StringRef(4))]
    );
    assert_eq!(frame.status, Some(StringRef(3)));
    assert_eq!(frame.parameters[0].name, StringRef(2));
    assert_eq!(frame.locals[0].name, StringRef(3));
    assert_eq!(frame.instructions[0].operands.len(), 6);
    assert_eq!(frame.instructions[0].operands[5], Operand::Boolean(255));
    assert_eq!(
        data.messages[0].message.as_ref().unwrap().variables,
        [Value::Integer(42)]
    );
    assert_eq!(data.messages[1].id, None);
    assert_eq!(data.messages[2].id, Some(999));
    assert_eq!(data.unknown_ids, [u64::MAX]);
    assert_eq!(data.unbinds[0].id, 0x1_0000_0000);
    assert_eq!(data.to_bytes().unwrap(), bytes);
}

#[test]
fn truncated_execution_never_returns_a_partial_success() {
    let bytes = fixture();
    for end in 0..bytes.len() {
        assert!(read(&bytes[..end]).is_err(), "accepted truncation at {end}");
    }
}

#[test]
fn active_data_requires_each_header_id_once() {
    let mut bytes = fixture();
    bytes[..4].copy_from_slice(&99u32.to_le_bytes());
    assert!(read(&bytes)
        .unwrap_err()
        .to_string()
        .contains("active data ID"));
    bytes[..4].copy_from_slice(&1u32.to_le_bytes());
    assert!(read(&bytes)
        .unwrap_err()
        .to_string()
        .contains("active data ID"));
}

#[test]
fn malformed_instruction_counts_types_and_strings_fail() {
    let mut call = vec![23, 0, 0, 0, 3];
    for count in [u32::MAX, i32::MAX as u32, 1] {
        call.truncate(5);
        words(&mut call, &[count]);
        assert!(Instruction::read(&mut Cursor::new(&call, "test"), 5).is_err());
    }
    for bytes in [
        vec![48],
        vec![36],
        vec![26, 7],
        vec![26, 2, 5, 0, 0, 0],
        vec![23, 0, 0, 0, 0],
    ] {
        assert!(Instruction::read(&mut Cursor::new(&bytes, "test"), 5).is_err());
    }
}

#[test]
fn fragment_payloads_keep_fields_and_optional_variables() {
    let cases: &[(&[u8], &[u8])] = &[
        (b"QuestStage", &[0x40, 0, 1, 200, 0, 5]),
        (b"ScenePhaseResults", &[0x40, 0, 1, 1, 2, 3, 4]),
        (b"SceneActionResults", &[0x40, 0, 1, 1, 2, 3, 4]),
        (b"SceneResults", &[0x40, 0, 1]),
        (
            b"TerminalRunResults",
            &[6, 1, 2, 3, 4, 0x40, 0, 1, 2, 0, 0, 0],
        ),
        (b"TopicInfo", &[]),
    ];
    for &(name, payload) in cases {
        for kind in [1, 3] {
            let mut bytes = (name.len() as u32).to_le_bytes().to_vec();
            bytes.extend_from_slice(name);
            bytes.extend_from_slice(payload);
            if kind == 3 {
                bytes.extend_from_slice(&[3, 42, 0, 0, 0]);
            }
            let mut cursor = Cursor::new(&bytes, "test");
            let fragment = FragmentTask::read(&mut cursor, 5, kind).unwrap();
            assert_eq!(cursor.remaining(), 0);
            let mut written = Vec::new();
            fragment.write(&mut written, kind).unwrap();
            assert_eq!(written, bytes);
        }
    }
    let bytes = [3, 0, 0, 0, b'B', b'a', b'd'];
    assert!(FragmentTask::read(&mut Cursor::new(&bytes, "test"), 5, 1).is_err());
}

#[test]
fn diagnostic_encoder_refuses_inconsistent_execution_fields() {
    let mut data = read(&fixture()).unwrap();
    data.active_data[0].trailing = None;
    assert!(data.to_bytes().is_err());
    let mut data = read(&fixture()).unwrap();
    data.active_data[0].frames[0].instructions[0].operands.pop();
    assert!(data.to_bytes().is_err());
    let mut data = read(&fixture()).unwrap();
    data.messages[1].id = Some(2);
    assert!(data.to_bytes().is_err());
}
