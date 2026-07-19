//! `quinte completions <bash|zsh|fish>` — 手写补全脚本（静态字符串，无外部 crate）。

const SUBCOMMANDS: &str = "init status doctor run wait resume cancel inspect primary-arbiter agents policy credential brief completions";
const COMMON_FLAGS: &str = "--brief --wait --json --response --verdict --force --home";

pub const BASH: &str = r#"# quinte bash completion — 安装：quinte completions bash > ~/.local/share/bash-completion/completions/quinte
_quinte() {
    local cur prev words cword
    _init_completion -n : || return
    local subcommands="SUBCOMMANDS"
    local flags="FLAGS"
    if [ "$cword" -eq 1 ]; then
        COMPREPLY=($(compgen -W "$subcommands" -- "$cur"))
        return 0
    fi
    case "${words[1]}" in
        primary-arbiter)
            if [ "$cword" -eq 2 ]; then
                COMPREPLY=($(compgen -W "request submit" -- "$cur"))
                return 0
            fi
            ;;
        agents)
            if [ "$cword" -eq 2 ]; then
                COMPREPLY=($(compgen -W "list describe" -- "$cur"))
                return 0
            fi
            ;;
        policy)
            if [ "$cword" -eq 2 ]; then
                COMPREPLY=($(compgen -W "show validate" -- "$cur"))
                return 0
            fi
            ;;
        credential)
            if [ "$cword" -eq 2 ]; then
                COMPREPLY=($(compgen -W "status" -- "$cur"))
                return 0
            fi
            ;;
        brief)
            if [ "$cword" -eq 2 ]; then
                COMPREPLY=($(compgen -W "new validate" -- "$cur"))
                return 0
            elif [ "$cword" -eq 3 ] && [ "${words[2]}" = "new" ]; then
                COMPREPLY=($(compgen -W "--print-template" -- "$cur"))
                return 0
            fi
            ;;
        completions)
            if [ "$cword" -eq 2 ]; then
                COMPREPLY=($(compgen -W "bash zsh fish" -- "$cur"))
                return 0
            fi
            ;;
    esac
    if [[ "$cur" == -* ]]; then
        COMPREPLY=($(compgen -W "$flags" -- "$cur"))
    else
        COMPREPLY=($(compgen -f -- "$cur"))
    fi
    return 0
}
complete -F _quinte quinte
"#;

pub const ZSH: &str = r#"#compdef quinte
# quinte zsh completion — 安装：quinte completions zsh > ~/.zfunc/_quinte && echo 'fpath=(~/.zfunc $fpath); autoload -Uz compinit && compinit' >> ~/.zshrc
_quinte() {
    local -a subcommands flags
    subcommands=(SUBCOMMANDS)
    flags=(FLAGS)
    if (( CURRENT == 2 )); then
        _describe 'subcommand' subcommands
        return
    fi
    case "$words[2]" in
        primary-arbiter)
            (( CURRENT == 3 )) && compadd request submit && return ;;
        agents)
            (( CURRENT == 3 )) && compadd list describe && return ;;
        policy)
            (( CURRENT == 3 )) && compadd show validate && return ;;
        credential)
            (( CURRENT == 3 )) && compadd status && return ;;
        brief)
            if (( CURRENT == 3 )); then
                compadd new validate && return
            elif (( CURRENT == 4 )) && [[ "$words[3]" == new ]]; then
                compadd -- --print-template && return
            fi ;;
        completions)
            (( CURRENT == 3 )) && compadd bash zsh fish && return ;;
    esac
    if [[ "$PREFIX" == -* ]]; then
        _describe 'flag' flags
    else
        _files
    fi
}
_quinte "$@"
"#;

pub const FISH: &str = r#"# quinte fish completion — 安装：quinte completions fish > ~/.config/fish/completions/quinte.fish
# 覆盖 flags: --brief --wait --json --response --verdict --force --home（fish 写法为 -l <name>）
function __fish_quinte_needs_command
    set -l cmd (commandline -opc)
    test (count $cmd) -eq 1
end
function __fish_quinte_using_command
    set -l cmd (commandline -opc)
    test (count $cmd) -gt 1; and test $argv[1] = $cmd[2]
end
for sub in SUBCOMMANDS_LIST
    complete -f -c quinte -n __fish_quinte_needs_command -a $sub
end
complete -f -c quinte -n '__fish_quinte_using_command primary-arbiter' -a 'request submit'
complete -f -c quinte -n '__fish_quinte_using_command agents' -a 'list describe'
complete -f -c quinte -n '__fish_quinte_using_command policy' -a 'show validate'
complete -f -c quinte -n '__fish_quinte_using_command credential' -a 'status'
complete -f -c quinte -n '__fish_quinte_using_command brief' -a 'new validate'
complete -f -c quinte -n '__fish_quinte_using_command completions' -a 'bash zsh fish'
complete -f -c quinte -l brief -r
complete -f -c quinte -l wait
complete -f -c quinte -l json
complete -f -c quinte -l response -r
complete -f -c quinte -l verdict -r
complete -f -c quinte -l force
complete -f -c quinte -l home -r
complete -f -c quinte -l print-template
"#;

pub fn render(shell: &str) -> Option<String> {
    match shell {
        "bash" => Some(
            BASH.replace("SUBCOMMANDS", SUBCOMMANDS)
                .replace("FLAGS", COMMON_FLAGS),
        ),
        "zsh" => Some(
            ZSH.replace("SUBCOMMANDS", SUBCOMMANDS)
                .replace("FLAGS", COMMON_FLAGS),
        ),
        "fish" => Some(FISH.replace("SUBCOMMANDS_LIST", &fish_subcommand_list())),
        _ => None,
    }
}

fn fish_subcommand_list() -> String {
    SUBCOMMANDS
        .split(' ')
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn install_hint(shell: &str) -> String {
    match shell {
        "bash" => "# 安装：quinte completions bash > ~/.local/share/bash-completion/completions/quinte".into(),
        "zsh" => "# 安装：quinte completions zsh > ~/.zfunc/_quinte（确保 fpath 含 ~/.zfunc 且 compinit 已启用）".into(),
        "fish" => "# 安装：quinte completions fish > ~/.config/fish/completions/quinte.fish".into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_shells_cover_every_subcommand() {
        for shell in ["bash", "zsh", "fish"] {
            let script = render(shell).expect("known shell");
            for sub in [
                "init",
                "status",
                "doctor",
                "run",
                "wait",
                "resume",
                "cancel",
                "inspect",
                "primary-arbiter",
                "agents",
                "policy",
                "credential",
                "brief",
                "completions",
            ] {
                assert!(script.contains(sub), "{shell} 缺 {sub}");
            }
            for flag in ["--brief", "--wait", "--json", "--response", "--verdict"] {
                assert!(script.contains(flag), "{shell} 缺 {flag}");
            }
        }
        assert!(render("powershell").is_none());
    }
}
