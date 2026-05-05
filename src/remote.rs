use std::fs;
use crate::secure;

pub const REMOTE_PATH: &str = ".browsercfg/remote";

fn parse_script(script: &serde_json::Value) {
    let mut curr_path = REMOTE_PATH.to_string();
    for action in script.as_array().expect("err: cannot represent script as array") {
        let sanitized_type = secure::sanitize_str(
            action[0].as_str().expect("err: missing necessary start"),
            true
        );
        let type_str = sanitized_type.as_str();

        let args: Vec<&str> = action.as_array()
            .expect("err: cannot represent action as array")[1..]
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        // {[0-9]l?} is the parameter matching design. l represents loose.
        let command_args: Vec<String> = match type_str {
            "git_clone" => ["git", "clone", "{1l}", "{2}"],
            "git_pull" => ["git", "pull", "{1}", "{2}"],
            "cd" => {
                let unsanitized_path = args.first().copied().unwrap_or("");
                let new_path = secure::sanitize_str(unsanitized_path, true);
                curr_path = format!("{REMOTE_PATH}/{new_path}");
                continue;
            },
            _ => {
                panic!("err: unknown action {type_str}");
            }
        }
        .to_vec()
        .iter()
        .map(|arg| secure::arg_replace(arg, &args))
        .collect();

        let mut command = std::process::Command::new(&command_args[0]);
        command.args(&command_args[1..]);
        let abs_path = fs::canonicalize(&curr_path).expect("err: could not get absolute path to folder");
        command.current_dir(abs_path);
        command.status().expect("err: failed to execute command");
    }
}

fn trigger_remote(remote_config: &serde_json::Value) {
    let remote_has_contents = fs::read_dir(REMOTE_PATH)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if remote_has_contents {
        let update_script = remote_config.get("update");
        if update_script.is_some() {
            parse_script(update_script.expect("err: impossible"));
        }
    } else {
        let _ = fs::create_dir_all(REMOTE_PATH);
        let init_script = remote_config.get("init");
        if init_script.is_some() {
            parse_script(init_script.expect("err: impossible"));
        }
    }
}

pub fn check_status(source: String, config: &serde_json::Value) {
    let remote_config = config.get("remote");
    if remote_config.is_some() {
        let definite_config = remote_config.expect("err: impossible");
        let run_on = definite_config.get("run_on");
        let source_as_value = &serde_json::Value::String(source);
        if run_on.is_some() &&
            run_on.expect("err: impossible").as_array().unwrap()
                .contains(source_as_value) {
            trigger_remote(definite_config);
        }
    }
}