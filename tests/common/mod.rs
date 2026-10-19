#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

/// 每个用例独立的 PJMAN_HOME 与工作根目录
pub struct Harness {
    pub home: TempDir,
    pub root: TempDir,
}

impl Harness {
    pub fn new() -> Self {
        Self {
            home: TempDir::new().unwrap(),
            root: TempDir::new().unwrap(),
        }
    }

    /// 在 root 下运行 pjman（继承 home，关闭 RUST_LOG）
    pub fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("pjman").unwrap();
        c.env("PJMAN_HOME", self.home.path());
        c.env_remove("RUST_LOG");
        c.current_dir(self.root.path());
        c
    }

    fn output(&self, args: &[&str]) -> std::process::Output {
        self.cmd()
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("执行 {args:?} 失败：{e}"))
    }

    pub fn run(&self, args: &[&str]) -> String {
        let output = self.output(args);
        assert!(
            output.status.success(),
            "命令应成功：{args:?}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    /// 预期失败，返回 (code, stdout, stderr)
    pub fn run_fail(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let output = self.output(args);
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    pub fn run_json(&self, args: &[&str]) -> Value {
        let mut full = args.to_vec();
        full.push("--json");
        let out = self.run(&full);
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("JSON 解析失败：{e}\n输出：{out}"))
    }

    pub fn root_path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
}

/// JSON 数组中取指定 name 的对象
pub fn find_by_name<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    value.as_array()?.iter().find(|v| v["name"] == name)
}

pub fn path_exists<P: AsRef<Path>>(p: P) -> bool {
    p.as_ref().exists()
}
