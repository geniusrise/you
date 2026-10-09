pub fn cron_lines(root: &std::path::Path) -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "aiyou".into());
    format!("# aiyou: harvest chats daily\n0 3 * * * AIYOU_HOME={root} {exe} sync\n# aiyou: refresh profile weekly if new chats\n0 4 * * 0 AIYOU_HOME={root} {exe} profile --if-changed\n", root = root.display(), exe = exe)
}
