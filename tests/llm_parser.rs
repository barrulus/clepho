use clepho::llm::client::parse_describe_and_tag_response;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/llm_responses")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|_| panic!("missing fixture: {:?}", p))
}

#[test]
fn parses_direct_json() {
    let (d, t) = parse_describe_and_tag_response(&fixture("valid.json"));
    assert_eq!(d, "A sunset over Rome.");
    assert_eq!(t, vec!["sunset", "rome", "architecture"]);
}

#[test]
fn parses_code_fenced_json() {
    let (d, t) = parse_describe_and_tag_response(&fixture("code_fenced.json"));
    assert_eq!(d, "A sunset over Rome.");
    assert_eq!(t, vec!["sunset", "rome"]);
}

#[test]
fn parses_legacy_tags_format() {
    let (d, t) = parse_describe_and_tag_response(&fixture("legacy_tags.txt"));
    assert_eq!(d, "A sunset over Rome.");
    assert_eq!(t, vec!["sunset", "rome", "architecture"]);
}

#[test]
fn malformed_returns_full_text_and_empty_tags() {
    let raw = fixture("malformed.txt");
    let (d, t) = parse_describe_and_tag_response(&raw);
    assert_eq!(d.trim(), raw.trim());
    assert!(t.is_empty());
}

#[test]
fn empty_tags_array_is_preserved() {
    let (d, t) = parse_describe_and_tag_response(&fixture("empty_tags.json"));
    assert_eq!(d, "Nothing notable.");
    assert!(t.is_empty());
}

#[test]
fn json_with_extra_unknown_fields_still_parses() {
    let raw = r#"{"description": "x", "tags": ["a"], "extra": 42}"#;
    let (d, t) = parse_describe_and_tag_response(raw);
    assert_eq!(d, "x");
    assert_eq!(t, vec!["a"]);
}

#[test]
fn legacy_tags_lowercases_and_filters_empty() {
    let raw = "Description here.\n\n  TAGS: Sunset, , ROME ,architecture\n";
    let (d, t) = parse_describe_and_tag_response(raw);
    assert_eq!(d, "Description here.");
    assert_eq!(t, vec!["sunset", "rome", "architecture"]);
}
