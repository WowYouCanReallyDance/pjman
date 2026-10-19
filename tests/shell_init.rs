mod common;

use common::Harness;

#[test]
fn init_scripts_are_syntactically_valid() {
    let h = Harness::new();

    let bash = h.run(&["init", "bash"]);
    std::fs::write(h.root.path().join("init.bash"), bash).unwrap();
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(h.root.path().join("init.bash"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bash 语法错误：{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let zsh = h.run(&["init", "zsh"]);
    std::fs::write(h.root.path().join("init.zsh"), zsh).unwrap();
    let out = std::process::Command::new("zsh")
        .arg("-n")
        .arg(h.root.path().join("init.zsh"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "zsh 语法错误：{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn completion_scripts_generate() {
    let h = Harness::new();
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let out = h.run(&["completion", shell]);
        assert!(!out.is_empty(), "{shell} 补全脚本不应为空");
    }
}
