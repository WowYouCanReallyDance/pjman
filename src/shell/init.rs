//! `pjman init <shell>` 输出的包装函数模板。
//!
//! 子进程无法改变父 shell 的 cwd，因此包装函数拦截
//! `ws/pj jump|jp`，改为 `cd $(pjman ... jump --print ...)`，
//! 其余参数原样转发给二进制。

pub fn wrapper(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        "powershell" => Some(POWERSHELL),
        _ => None,
    }
}

const BASH: &str = r#"# pjman shell integration (bash)
pjman() {
  case "$1" in
    ws|workspace|pj|project)
      if [ "$2" = "jump" ] || [ "$2" = "jp" ]; then
        local dir
        dir=$(command pjman "$1" jump --print "${@:3}") || return $?
        if [ -n "$dir" ]; then cd "$dir" || return; fi
      else
        command pjman "$@"
      fi
      ;;
    *)
      command pjman "$@"
      ;;
  esac
}
"#;

const ZSH: &str = r#"# pjman shell integration (zsh)
pjman() {
  case "$1" in
    ws|workspace|pj|project)
      if [ "$2" = "jump" ] || [ "$2" = "jp" ]; then
        local dir
        dir=$(command pjman "$1" jump --print "${@:3}") || return $?
        if [ -n "$dir" ]; then cd "$dir" || return; fi
      else
        command pjman "$@"
      fi
      ;;
    *)
      command pjman "$@"
      ;;
  esac
}
"#;

const FISH: &str = r#"# pjman shell integration (fish)
function pjman
    switch $argv[1]
        case ws workspace pj project
            if test "$argv[2]" = jump
                or test "$argv[2]" = "jp"
                set -l dir (command pjman $argv[1] jump --print $argv[3..-1])
                or return 1
                if test -n "$dir"
                    cd "$dir"
                end
            else
                command pjman $argv
            end
        case '*'
            command pjman $argv
    end
end
"#;

const POWERSHELL: &str = r#"# pjman shell integration (powershell)
function pjman {
    param([Parameter(ValueFromRemainingArguments = $true)] $Args)
    if ($Args.Count -ge 2 -and
        ($Args[0] -eq 'ws' -or $Args[0] -eq 'workspace' -or $Args[0] -eq 'pj' -or $Args[0] -eq 'project') -and
        ($Args[1] -eq 'jump' -or $Args[1] -eq 'jp')) {
        $dir = & pjman.exe $Args[0] jump --print $Args[2..($Args.Count - 1)]
        if ($LASTEXITCODE -eq 0 -and $dir) { Set-Location $dir }
    } else {
        & pjman.exe @Args
    }
}
"#;
