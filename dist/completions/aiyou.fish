# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_aiyou_global_optspecs
    string join \n root= shell= h/help V/version
end

function __fish_aiyou_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_aiyou_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_aiyou_using_subcommand
    set -l cmd (__fish_aiyou_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c aiyou -n "__fish_aiyou_needs_command" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_needs_command" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_needs_command" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_needs_command" -s V -l version -d 'Print version'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "init" -d 'Create the store, config and git repo'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "sync" -d 'Harvest chats from local harnesses (run from cron to keep updating)'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "import" -d 'Import a dump from ChatGPT / Claude / Gemini (re-import newer dumps safely)'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "profile" -d 'Run the LLM swarm over all chats to (re)build your profile + SKILL.md'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "profile-install" -d 'Install the generated SKILL.md into a harness skill directory'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "status" -d 'Show per-source counts, last sync and last profile info'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "cron" -d 'Print crontab lines for scheduled sync + profile'
complete -c aiyou -n "__fish_aiyou_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c aiyou -n "__fish_aiyou_using_subcommand init" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand init" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_using_subcommand init" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_using_subcommand sync" -l source -d 'Restrict to these sources (repeatable)' -r
complete -c aiyou -n "__fish_aiyou_using_subcommand sync" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand sync" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_using_subcommand sync" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_using_subcommand import" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand import" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_using_subcommand import" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_using_subcommand profile" -l model -d 'Override model from config' -r
complete -c aiyou -n "__fish_aiyou_using_subcommand profile" -l workers -d 'Override worker count from config' -r
complete -c aiyou -n "__fish_aiyou_using_subcommand profile" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand profile" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_using_subcommand profile" -l if-changed -d 'Skip if no chats changed since last run (cron-friendly)'
complete -c aiyou -n "__fish_aiyou_using_subcommand profile" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_using_subcommand profile-install" -l target -d 'One of: claude, opencode, crush, codex' -r
complete -c aiyou -n "__fish_aiyou_using_subcommand profile-install" -l agents-dir -d 'Custom agents/skills directory' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand profile-install" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand profile-install" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_using_subcommand profile-install" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_using_subcommand status" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand status" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_using_subcommand status" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_using_subcommand cron" -l root -d 'Store root (default: $AIYOU_HOME or ~/.aiyou)' -r -F
complete -c aiyou -n "__fish_aiyou_using_subcommand cron" -l shell -d 'Print shell completions for the given shell and exit' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c aiyou -n "__fish_aiyou_using_subcommand cron" -s h -l help -d 'Print help'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "init" -d 'Create the store, config and git repo'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "sync" -d 'Harvest chats from local harnesses (run from cron to keep updating)'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "import" -d 'Import a dump from ChatGPT / Claude / Gemini (re-import newer dumps safely)'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "profile" -d 'Run the LLM swarm over all chats to (re)build your profile + SKILL.md'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "profile-install" -d 'Install the generated SKILL.md into a harness skill directory'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "status" -d 'Show per-source counts, last sync and last profile info'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "cron" -d 'Print crontab lines for scheduled sync + profile'
complete -c aiyou -n "__fish_aiyou_using_subcommand help; and not __fish_seen_subcommand_from init sync import profile profile-install status cron help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
