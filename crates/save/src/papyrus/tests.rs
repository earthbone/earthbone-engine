// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::cursor::WString;

const FIRST: u64 = 0x1_0000_0001;
const SECOND: u64 = 0x2_0000_0002;
const REFERENCE: u64 = 0x3_0000_0003;
const INTS: u64 = 0x4_0000_0004;
const OBJECTS: u64 = 0x5_0000_0005;

fn u32_bytes(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn u64_bytes(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn data_header(out: &mut Vec<u8>, id: u64, flags: u8, label: u32, count: u32) {
    u64_bytes(out, id);
    out.push(flags);
    u32_bytes(out, label);
    u32_bytes(out, 99);
    if flags & 4 != 0 {
        u32_bytes(out, 123);
    }
    u32_bytes(out, count);
}

fn fixture() -> (Vec<u8>, usize, usize) {
    let mut out = 2u16.to_le_bytes().to_vec();
    u32_bytes(&mut out, 5);
    for text in ["", "Script", "Value", "int", "Active"] {
        WString::from_text(text).write(&mut out);
    }
    u32_bytes(&mut out, 1);
    for value in [1, 0, 1, 2, 3] {
        u32_bytes(&mut out, value);
    }
    u32_bytes(&mut out, 2);
    let instance_start = out.len();
    for id in [FIRST, SECOND] {
        u64_bytes(&mut out, id);
        u32_bytes(&mut out, 1);
        out.extend_from_slice(&[0, 0, 0, 0, 0x40, 0, 0x14, 0]);
    }
    u32_bytes(&mut out, 1);
    u64_bytes(&mut out, REFERENCE);
    u32_bytes(&mut out, 1);
    u32_bytes(&mut out, 2);
    u64_bytes(&mut out, INTS);
    out.push(3);
    u32_bytes(&mut out, 3);
    u64_bytes(&mut out, OBJECTS);
    out.push(1);
    u32_bytes(&mut out, 1);
    u32_bytes(&mut out, 1);
    u32_bytes(&mut out, 7);
    u32_bytes(&mut out, 1);
    u32_bytes(&mut out, 0x0102_0304);
    out.push(2);
    let data_start = out.len();
    // Payload order differs from the first-pass headers.
    data_header(&mut out, SECOND, 4, 4, 1);
    out.push(13);
    u64_bytes(&mut out, INTS);
    data_header(&mut out, FIRST, 0, 0, 9);
    out.push(0);
    u32_bytes(&mut out, 0xdead_beef);
    out.push(1);
    u32_bytes(&mut out, 1);
    u64_bytes(&mut out, SECOND);
    out.push(2);
    u32_bytes(&mut out, 2);
    out.push(3);
    u32_bytes(&mut out, (-7i32) as u32);
    out.push(4);
    u32_bytes(&mut out, 0x7fc0_0123);
    out.push(5);
    u32_bytes(&mut out, u32::MAX);
    out.extend_from_slice(&[6, 3]);
    u32_bytes(&mut out, 42);
    out.push(7);
    u32_bytes(&mut out, 1);
    u64_bytes(&mut out, 0);
    out.push(11);
    u32_bytes(&mut out, 1);
    u64_bytes(&mut out, OBJECTS);
    data_header(&mut out, REFERENCE, 0, 1, 1);
    out.push(2);
    u32_bytes(&mut out, 2);
    u64_bytes(&mut out, OBJECTS);
    out.push(1);
    u32_bytes(&mut out, 1);
    u64_bytes(&mut out, 999);
    u64_bytes(&mut out, INTS);
    for value in [1, 2, 3] {
        out.push(3);
        u32_bytes(&mut out, value);
    }
    u32_bytes(&mut out, 0x0102_0304);
    out.extend_from_slice(&[1, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
    u32_bytes(&mut out, 0);
    for _ in 0..6 {
        u32_bytes(&mut out, 0);
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(b"opaque auxiliary tail");
    (out, data_start, instance_start)
}

#[test]
fn second_pass_ids_values_and_bit_patterns_are_preserved() {
    let (bytes, start, _) = fixture();
    let p = Papyrus::read(&bytes).unwrap();
    assert_eq!(p.data_start, start);
    assert_eq!(p.instance_data[0].id, SECOND);
    assert_eq!(p.instance_data[0].extra, Some(123));
    assert_eq!(p.string(p.instance_data[0].state_or_type), Some("Active"));
    assert_eq!(p.active_scripts[0].id, 0x0102_0304);
    assert_eq!(p.runtime_id, 7);
    let values = &p.instance_data[1].variables;
    assert_eq!(values[0], Value::Null(0xdead_beef));
    assert_eq!(
        values[1],
        Value::Reference {
            type_name: StringRef(1),
            id: SECOND
        }
    );
    assert_eq!(values[3], Value::Integer(-7));
    let Value::Float(float) = values[4] else {
        panic!("expected float")
    };
    assert_eq!(float.to_bits(), 0x7fc0_0123);
    assert_eq!(values[5], Value::Boolean(u32::MAX));
    assert_eq!(values[6], Value::Variant(Box::new(Value::Integer(42))));
    assert_eq!(p.array_data[0].id, OBJECTS);
    assert_eq!(
        p.array_data[1].variables,
        vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)]
    );
    assert_eq!(
        p.value_data_bytes().unwrap(),
        bytes[p.data_start..p.execution_start]
    );
    assert_eq!(
        p.execution.to_bytes().unwrap(),
        bytes[p.execution_start..p.decoded_len]
    );
    assert_eq!(&bytes[p.decoded_len..], b"opaque auxiliary tail");
}

#[test]
fn every_truncated_prefix_fails_before_the_raw_tail() {
    let (bytes, _, _) = fixture();
    let end = Papyrus::read(&bytes).unwrap().decoded_len;
    for n in 0..end {
        assert!(
            Papyrus::read(&bytes[..n]).is_err(),
            "accepted truncation at {n}"
        );
    }
}

#[test]
fn duplicate_or_unknown_ids_are_not_assigned_to_other_objects() {
    let (mut bytes, data_start, instance_start) = fixture();
    bytes[instance_start + 20..instance_start + 28].copy_from_slice(&FIRST.to_le_bytes());
    assert!(Papyrus::read(&bytes).is_err());
    let (mut bytes, _, _) = fixture();
    bytes[data_start..data_start + 8].copy_from_slice(&999u64.to_le_bytes());
    assert!(Papyrus::read(&bytes).is_err());
    let (mut bytes, _, _) = fixture();
    bytes[data_start..data_start + 8].copy_from_slice(&FIRST.to_le_bytes());
    assert!(Papyrus::read(&bytes).is_err());
}

#[test]
fn array_value_tags_preserve_type_names_and_64_bit_ids() {
    for tag in 11..=17 {
        let mut bytes = vec![tag];
        if tag == 11 || tag == 17 {
            u32_bytes(&mut bytes, 4);
        }
        u64_bytes(&mut bytes, u64::MAX);
        let mut cursor = Cursor::new(&bytes, "test");
        let value = Value::read(&mut cursor, 5, 0).unwrap();
        assert_eq!(cursor.remaining(), 0);
        let Value::Array {
            element_type,
            type_name,
            id,
        } = value
        else {
            panic!("expected array")
        };
        assert_eq!(element_type as u8, tag - 10);
        assert_eq!(
            type_name,
            if tag == 11 || tag == 17 {
                Some(StringRef(4))
            } else {
                None
            }
        );
        assert_eq!(id, u64::MAX);
        let mut rewritten = Vec::new();
        value.write(&mut rewritten).unwrap();
        assert_eq!(rewritten, bytes);
    }
}

#[test]
fn invalid_string_indices_counts_tags_and_recursive_variants_fail() {
    let (mut bytes, _, instance_start) = fixture();
    bytes[instance_start + 8..instance_start + 12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Papyrus::read(&bytes).is_err());
    let (mut bytes, _, _) = fixture();
    bytes[2..6].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Papyrus::read(&bytes).is_err());
    for tag in [8, 9, 10, 18, 255] {
        assert!(Value::read(&mut Cursor::new(&[tag], "test"), 5, 0).is_err());
    }
    let mut nested = vec![6; 129];
    nested.extend_from_slice(&[0, 0, 0, 0, 0]);
    assert!(Value::read(&mut Cursor::new(&nested, "test"), 5, 0).is_err());
}
