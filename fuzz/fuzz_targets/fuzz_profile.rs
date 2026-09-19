#![no_main]

//! Profile text parsing, including lossy input and serializer stability.

use libfuzzer_sys::fuzz_target;
use soe_profile::ini::Ini;
use soe_profile::plugins_txt::{
    parse_loadorder_txt, parse_plugins_txt, render_loadorder_txt, render_plugins_txt,
};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let parsed = Ini::parse(&text);
    let _ = Ini::parse(&parsed.render());
    // Unnamed sections are accepted on input but have no rendered header.
    // Only named sections have a lossless round-trip contract here.
    let mut named = parsed;
    named.sections.retain(|section| !section.name.is_empty());
    let ini = named.render();
    assert_eq!(Ini::parse(&ini).render(), ini);
    let plugins: Vec<_> = parse_plugins_txt(&text)
        .into_iter()
        .filter(|p| representable(&p.name))
        .collect();
    assert_eq!(parse_plugins_txt(&render_plugins_txt(&plugins)), plugins);
    let order: Vec<_> = parse_loadorder_txt(&text)
        .into_iter()
        .filter(|name| representable(name))
        .collect();
    assert_eq!(parse_loadorder_txt(&render_loadorder_txt(&order)), order);
});

fn representable(name: &str) -> bool {
    !name.is_empty() && !name.starts_with(['*', '#', '\u{feff}']) && name.trim() == name
}
