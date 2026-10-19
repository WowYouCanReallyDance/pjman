use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::{Cli, InitShell};
use crate::error::AppResult;
use crate::shell::init::wrapper;

pub fn run(shell: InitShell) -> AppResult<()> {
    let (name, comp_shell) = match shell {
        InitShell::Bash => ("bash", Shell::Bash),
        InitShell::Zsh => ("zsh", Shell::Zsh),
        InitShell::Fish => ("fish", Shell::Fish),
        InitShell::Powershell => ("powershell", Shell::PowerShell),
    };

    print!("{}", wrapper(name).expect("shell 模板缺失"));

    // 拼接静态补全脚本，eval 一次同时安装包装函数与补全
    let mut buf = Vec::new();
    generate(comp_shell, &mut Cli::command(), "pjman", &mut buf);
    println!("{}", String::from_utf8_lossy(&buf));

    match shell {
        InitShell::Bash => eprintln!("\n# 安装：echo 'eval \"$(pjman init bash)\"' >> ~/.bashrc"),
        InitShell::Zsh => eprintln!("\n# 安装：echo 'eval \"$(pjman init zsh)\"' >> ~/.zshrc"),
        InitShell::Fish => {
            eprintln!("\n# 安装：pjman init fish | source")
        }
        InitShell::Powershell => {
            eprintln!(
                "\n# 安装：Add-Content $PROFILE 'pjman init powershell | Out-String | Invoke-Expression'"
            )
        }
    }
    Ok(())
}
