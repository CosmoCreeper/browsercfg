use std::sync::LazyLock;
use std::fs;

static ENV_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\{[0-9]\}").unwrap()
});

fn arg_replace(string: &str, args: &Vec<&str>) -> String {
    ENV_REGEX.replace_all(string, |caps: &regex::Captures| {
        let param_str = caps.get(0).unwrap().as_str();

        let stripped = &param_str[1..param_str.len() - 1];

        let arg_num = stripped
            .chars()
            .next()
            .and_then(|ch| ch.to_digit(10))
            .expect("err: invalid argument placeholder") as usize - 1;

        let arg_val = args.get(arg_num).unwrap_or(&"");

        assert_safe(arg_val);
        arg_val.to_string()
    }).to_string()
}

pub fn parse_script(script: &serde_json::Value, path: String) {
    let mut curr_path = path.clone();
    for action in script.as_array().expect("err: cannot represent script as array") {
        let action_type = action[0].as_str().expect("err: missing necessary start argument");
        assert_safe(action_type);

        let args: Vec<&str> = action.as_array()
            .expect("err: cannot represent action as array")[1..]
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        // {[0-9]} is the parameter matching design
        let command_args: Vec<String> = match action_type {
            "git_clone" => vec!["git", "clone", "{1}", "{2}"],
            "git_pull" => vec!["git", "pull", "{1}", "{2}"],
            "git_checkout" => vec!["git", "checkout", "{1}", "{2}", "{3}"],
            "cd" => {
                let new_path = args.first().copied().unwrap_or("");
                assert_safe(new_path);
                // Every cd command is based off of the root path, so you never need "../"
                curr_path = format!("{path}/{new_path}");
                continue;
            },
            "rm" => {
                let file_path = args.first().copied().unwrap_or("");
                assert_safe(&file_path);
                let full_path = format!("{path}/{file_path}");
                fs::remove_file(&full_path).expect(&format!("err: failed to remove file at \"{full_path}\" for secure script"));
                continue;
            },
            "py_exec" => vec!["python3", "{1}"],
            _ => {
                panic!("err: unknown action {action_type}");
            }
        }
        .iter()
        .map(|arg| arg_replace(arg, &args))
        .collect();

        let mut command = std::process::Command::new(&command_args[0]);
        command.args(&command_args[1..]);
        let abs_path = fs::canonicalize(&curr_path).expect("err: could not get absolute path to folder");
        command.current_dir(abs_path);
        command.status().expect("err: failed to execute command");
    }
}

pub fn assert_safe(string: &str) {
    if string.contains(';') || string.contains('&') || string.contains(' ') || string.contains("..") {
        panic!("panic: String is unsafe! (contains ';', '&', ' ', or '..')");
    }
}

