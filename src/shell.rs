use std::fmt;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

impl fmt::Display for Shell {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Powershell => "powershell",
        };
        formatter.write_str(name)
    }
}

pub fn init_script(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => BASH_INIT,
        Shell::Zsh => ZSH_INIT,
        Shell::Fish => FISH_INIT,
        Shell::Powershell => POWERSHELL_INIT,
    }
}

const BASH_INIT: &str = r#"mkd() {
    if [ "$#" -gt 0 ]; then
        command mkd "$@"
        return $?
    fi

    local dir
    dir="$(command mkd __select)" || return $?
    if [ -n "$dir" ]; then
        builtin cd -- "$dir"
    fi
}
"#;

const ZSH_INIT: &str = r#"mkd() {
    if [ "$#" -gt 0 ]; then
        command mkd "$@"
        return $?
    fi

    local dir
    dir="$(command mkd __select)" || return $?
    if [ -n "$dir" ]; then
        builtin cd -- "$dir"
    fi
}
"#;

const FISH_INIT: &str = r#"function mkd
    if test (count $argv) -gt 0
        command mkd $argv
        return $status
    end

    set -l dir "$(command mkd __select)"
    set -l select_status $status
    if test $select_status -ne 0
        return $select_status
    end
    if test (string length -- "$dir") -gt 0
        builtin cd -- "$dir"
    end
end
"#;

const POWERSHELL_INIT: &str = r#"function mkd {
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]] $Arguments
    )

    $application = Get-Command mkd -CommandType Application | Select-Object -First 1
    if ($null -eq $application) {
        $global:LASTEXITCODE = 127
        return
    }

    if ($Arguments.Count -gt 0) {
        & $application.Path @Arguments
        $global:LASTEXITCODE = $LASTEXITCODE
        return
    }

    $dir = (& $application.Path __select | Out-String).TrimEnd([char[]]"`r`n")
    $selectStatus = $LASTEXITCODE
    if ($selectStatus -ne 0) {
        $global:LASTEXITCODE = $selectStatus
        return
    }
    if (-not [string]::IsNullOrEmpty($dir)) {
        Set-Location -LiteralPath $dir
    }
}
"#;
