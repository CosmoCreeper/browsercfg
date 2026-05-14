use std::fs;
use crate::secure;

pub const REMOTE_PATH: &str = ".browsercfg/remote";

fn trigger_remote(remote_config: &serde_json::Value) {
    let remote_has_contents = fs::read_dir(REMOTE_PATH)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if remote_has_contents {
        let update_script = remote_config.get("update");
        if update_script.is_some() {
            secure::parse_script(update_script.expect("err: impossible"), REMOTE_PATH.to_string());
        }
    } else {
        let _ = fs::create_dir_all(REMOTE_PATH);
        let init_script = remote_config.get("init");
        if init_script.is_some() {
            secure::parse_script(init_script.expect("err: impossible"), REMOTE_PATH.to_string());
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
