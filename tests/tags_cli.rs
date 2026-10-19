mod common;

use common::Harness;

fn key_entry<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    value.as_array()?.iter().find(|v| v["key"] == key)
}

fn values_of(entry: &serde_json::Value) -> Vec<&str> {
    entry["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

#[test]
fn escaped_comma_value_and_repeated_args() {
    let h = Harness::new();

    // 反斜杠转义值内逗号
    h.run(&["tags", "add", r"Authors=Matt Long\,Taylor Swift"]);
    let value = h.run_json(&["tags", "ls"]);
    let authors = key_entry(&value, "Authors").expect("Authors key 存在");
    assert_eq!(authors["values"][0], "Matt Long,Taylor Swift");

    // 重复参数取并集
    h.run(&["tags", "add", "K=one"]);
    h.run(&["tags", "add", "K=two"]);
    let value = h.run_json(&["tags", "ls"]);
    let values = values_of(key_entry(&value, "K").unwrap());
    assert!(values.contains(&"one"));
    assert!(values.contains(&"two"));
}

#[test]
fn bare_values_registered_under_default_key() {
    let h = Harness::new();
    h.run(&["tags", "add", "OpenSrc", "Apache"]);
    let value = h.run_json(&["tags", "ls"]);
    let values = values_of(key_entry(&value, "DefaultTags").expect("DefaultTags 存在"));
    assert!(values.contains(&"OpenSrc"));
    assert!(values.contains(&"Apache"));
}
