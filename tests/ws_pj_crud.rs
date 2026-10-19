mod common;

use common::{Harness, find_by_name, path_exists};

#[test]
fn fresh_db_has_only_dvws() {
    let h = Harness::new();
    let value = h.run_json(&["ws", "ls"]);
    let arr = value.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let dvws = &arr[0];
    assert_eq!(dvws["name"], "dvws");
    assert!(dvws["root_path"].is_null());
    assert_eq!(dvws["is_builtin"], true);
}

#[test]
fn bare_create_becomes_default_and_hosts_projects() {
    let h = Harness::new();

    let created = h.run_json(&["ws", "create"]);
    let ws_name = created[0]["name"].as_str().unwrap().to_owned();
    assert!(
        ws_name.starts_with("ws-01"),
        "默认命名应是 ws-<uuidv7>：{ws_name}"
    );
    assert!(path_exists(h.root_path(&ws_name)));

    // 无选择参数：project 创建在默认 ws 中
    h.run(&["pj", "create", "app1", "--tags", "Lang=Rust"]);
    assert!(path_exists(h.root_path(&ws_name).join("app1")));

    let projects = h.run_json(&["pj", "ls", "--all"]);
    let app = find_by_name(&projects, "app1").expect("app1 应存在");
    assert_eq!(app["workspace"], ws_name);
}

#[test]
fn external_project_registers_to_dvws() {
    let h = Harness::new();
    let ext = h.root.path().join("external");
    std::fs::create_dir_all(&ext).unwrap();

    h.run(&["pj", "add", ext.to_str().unwrap(), "--name", "ext-pj"]);
    let projects = h.run_json(&["pj", "ls"]);
    let pj = find_by_name(&projects, "ext-pj").expect("外部项目应在列表");
    assert_eq!(pj["workspace"], "dvws");
}

#[test]
fn select_unique_ambiguous_and_none() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa", "--tags", "License=MIT", "Team=A"]);
    h.run(&["ws", "create", "wsb", "--tags", "License=MIT", "Team=B"]);

    // 唯一命中
    h.run(&["pj", "create", "a", "--select", "Team=A"]);
    let projects = h.run_json(&["pj", "ls", "--workspace", "wsa"]);
    assert!(find_by_name(&projects, "a").is_some());

    // 歧义：列出 DefaultTags 候选，非零退出
    let (code, _, stderr) = h.run_fail(&["pj", "create", "b", "--select", "License=MIT"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("DefaultTags=wsa"));
    assert!(stderr.contains("DefaultTags=wsb"));

    // 零命中
    let (code, _, stderr) = h.run_fail(&["pj", "create", "c", "--select", "Team=ZZZ"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("没有 workspace 满足选择器"));
}

#[test]
fn rename_syncs_default_tags() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa"]);
    h.run(&["pj", "create", "old-name", "--workspace", "wsa"]);
    h.run(&["pj", "rename", "old-name", "new-name", "--workspace", "wsa"]);

    let projects = h.run_json(&["pj", "ls", "--workspace", "wsa"]);
    assert!(find_by_name(&projects, "old-name").is_none());
    assert!(find_by_name(&projects, "new-name").is_some());

    // jump 仍可按新名称定位
    let path = h.run(&["pj", "jump", "new-name", "--workspace", "wsa", "--print"]);
    assert!(path.trim().ends_with("wsa/old-name"));
}

#[test]
fn remove_keeps_disk_by_default_purge_deletes() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa"]);
    h.run(&["pj", "create", "keep", "--workspace", "wsa"]);
    h.run(&["pj", "create", "gone", "--workspace", "wsa"]);

    // 默认只注销 DB，目录保留
    h.run(&["pj", "remove", "keep", "--workspace", "wsa"]);
    assert!(path_exists(h.root_path("wsa/keep")));

    // --purge -y 删除目录
    h.run(&[
        "pj",
        "remove",
        "gone",
        "--workspace",
        "wsa",
        "--purge",
        "--yes",
    ]);
    assert!(!path_exists(h.root_path("wsa/gone")));

    // ws 下有 project 时禁止删除
    h.run(&["pj", "create", "last", "--workspace", "wsa"]);
    let (_, _, stderr) = h.run_fail(&["ws", "remove", "wsa"]);
    assert!(stderr.contains("仍有"));
}

#[test]
fn move_project_between_workspaces_moves_directory() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa"]);
    h.run(&["ws", "create", "wsb"]);
    h.run(&["pj", "create", "mover", "--workspace", "wsa"]);
    assert!(path_exists(h.root_path("wsa/mover")));

    h.run(&["pj", "move", "mover", "--to", "wsb"]);
    assert!(path_exists(h.root_path("wsb/mover")));
    assert!(!path_exists(h.root_path("wsa/mover")));

    let projects = h.run_json(&["pj", "ls", "--workspace", "wsb"]);
    assert!(find_by_name(&projects, "mover").is_some());
}

#[test]
fn name_tag_is_protected_in_mutation() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa"]);
    h.run(&["pj", "create", "prot", "--workspace", "wsa"]);

    let (_, _, stderr) = h.run_fail(&[
        "pj",
        "update",
        "prot",
        "--workspace",
        "wsa",
        "--remove",
        "DefaultTags=prot",
    ]);
    assert!(stderr.contains("受保护"));
}

#[test]
fn dvws_cannot_be_removed_or_renamed() {
    let h = Harness::new();
    let (_, _, e1) = h.run_fail(&["ws", "remove", "dvws"]);
    assert!(e1.contains("不可删除"));
    let (_, _, e2) = h.run_fail(&["ws", "rename", "dvws", "x"]);
    assert!(e2.contains("虚拟"));
}

#[test]
fn tag_mutation_add_set_remove() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa"]);
    h.run(&["pj", "create", "t", "--workspace", "wsa", "--tags", "K=v1"]);

    h.run(&["pj", "update", "t", "--workspace", "wsa", "--add", "K=v2"]);
    let projects = h.run_json(&["pj", "ls", "--workspace", "wsa", "--tag", "K=v2"]);
    assert!(find_by_name(&projects, "t").is_some());

    // set 覆盖整个 key
    h.run(&["pj", "update", "t", "--workspace", "wsa", "--set", "K=only"]);
    let projects = h.run_json(&["pj", "ls", "--workspace", "wsa", "--tag", "K=v1"]);
    assert!(find_by_name(&projects, "t").is_none());
    let projects = h.run_json(&["pj", "ls", "--workspace", "wsa", "--tag", "K=only"]);
    assert!(find_by_name(&projects, "t").is_some());

    // remove 单值
    h.run(&[
        "pj",
        "update",
        "t",
        "--workspace",
        "wsa",
        "--remove",
        "K=only",
    ]);
    let projects = h.run_json(&["pj", "ls", "--workspace", "wsa"]);
    let tags = find_by_name(&projects, "t").unwrap()["tags"]
        .as_array()
        .unwrap();
    assert!(!tags.iter().any(|t| t["key"] == "K"));
}

#[test]
fn dict_remove_blocked_when_referenced() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa", "--tags", "Ref=1"]);
    let (_, _, stderr) = h.run_fail(&["tags", "remove", "Ref=1"]);
    assert!(stderr.contains("引用"));
    h.run(&["tags", "remove", "Ref=1", "--force"]);
    let out = h.run(&["tags", "ls"]);
    assert!(!out.contains("Ref"));
}

#[test]
fn dynamic_completion_lists_workspaces() {
    let h = Harness::new();
    h.run(&["ws", "create", "wsa"]);
    h.run(&["ws", "create", "wsb"]);
    let out = h.run(&["__complete", "ws", ""]);
    assert!(out.contains("wsa"));
    assert!(out.contains("wsb"));
    assert!(out.contains("dvws"));
}
