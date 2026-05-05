use std::sync::LazyLock;

static ENV_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\{[0-9]l?\}").unwrap()
});

pub fn arg_replace(string: &str, args: &Vec<&str>) -> String {
    if let Some(param) = ENV_REGEX.find(string) {
        let param_str = param.as_str();
        let stripped_param = &param_str[1..param_str.len() - 1];

        let arg_num = stripped_param
            .chars()
            .next()
            .and_then(|ch| ch.to_digit(10))
            .expect("invalid argument placeholder") as usize - 1;
        let arg_val = args[arg_num];
        let is_loose = stripped_param.len() == 2;

        let sanitized_arg = sanitize_str(arg_val, !is_loose);
        let mut result = string.to_string();
        result.replace_range(param.start()..param.end(), &sanitized_arg);
        return result;
    }
    string.to_string()
}

/* 
  strict -> enable for paths to prevent ../, disable for urls
*/
pub fn sanitize_str(string: &str, strict: bool) -> String {
    let sanitized_generic = string.replace(&[';', ' ', '&'][..], "");
    if strict {
        return sanitized_generic.replace('.', "");
    }
    sanitized_generic
}