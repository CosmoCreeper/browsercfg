use indexmap::IndexMap;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

mod remote;
mod secure;

#[cfg(windows)]
const EXE_SUFFIX: &str = ".exe";
#[cfg(unix)]
const EXE_SUFFIX: &str = "";

static IMPORT_PATH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^\{.+\}(/|$)").unwrap()
});

const CURR_VERSION: &str = "0.1.0";
const BROWSER_CONFIG: &str = ".browsercfg/curr_browser";
const BROWSERS_FOLDER: &str = ".browsercfg/browsers/";
const CONFIG_FILE: &str = ".browsercfg.json";

// Default prefs.js necessary to skip welcome and setup screens
const DEFAULT_PREFS: &str = "
  user_pref(\"zen.welcome-screen.seen\", true);
  user_pref(\"browser.shell.checkDefaultBrowser\", false);
";

static BROWSER_MAP: LazyLock<IndexMap<&str, IndexMap<&str, HashMap<&str, &str>>>> = LazyLock::new(
    || {
        IndexMap::from([
            (
                "firefox",
                IndexMap::from([
                    (
                        "stable",
                        HashMap::from([
                            (
                                "linux",
                                "https://download.mozilla.org/?product=firefox-latest&os=linux64&lang=en-US",
                            ),
                            (
                                "windows",
                                "https://download.mozilla.org/?product=firefox-latest&os=win64&lang=en-US",
                            ),
                        ]),
                    ),
                    (
                        "devedition",
                        HashMap::from([
                            (
                                "linux",
                                "https://download.mozilla.org/?product=firefox-devedition-latest&os=linux64&lang=en-US",
                            ),
                            (
                                "windows",
                                "https://download.mozilla.org/?product=firefox-devedition-latest&os=win64&lang=en-US",
                            ),
                        ]),
                    ),
                    (
                        "nightly",
                        HashMap::from([
                            (
                                "linux",
                                "https://download.mozilla.org/?product=firefox-nightly-latest&os=linux64&lang=en-US",
                            ),
                            (
                                "windows",
                                "https://download.mozilla.org/?product=firefox-nightly-latest&os=win64&lang=en-US",
                            ),
                        ]),
                    ),
                ]),
            ),
            (
                "zen",
                IndexMap::from([
                    (
                        "beta",
                        HashMap::from([
                            (
                                "linux",
                                "https://github.com/zen-browser/desktop/releases/latest/download/zen.linux-x86_64.tar.xz",
                            ),
                            (
                                "windows",
                                "https://github.com/zen-browser/desktop/releases/latest/download/zen.installer.exe",
                            ),
                        ]),
                    ),
                    (
                        "twilight",
                        HashMap::from([
                            (
                                "linux",
                                "https://github.com/zen-browser/desktop/releases/download/twilight-1/zen.linux-x86_64.tar.xz",
                            ),
                            (
                                "windows",
                                "https://github.com/zen-browser/desktop/releases/download/twilight-1/zen.installer.exe",
                            ),
                        ]),
                    ),
                ]),
            ),
        ])
    },
);

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>, ignore: &Vec<&str>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        
        let file_name = entry.file_name();
        if !ignore.contains(&file_name.to_str().unwrap_or("")) {
            if ty.is_dir() { 
                copy_dir_all(entry.path(), dst.as_ref().join(file_name), ignore)?;
            } else {
                fs::copy(entry.path(), dst.as_ref().join(file_name))?;
            }
        }
    }
    
    Ok(())
}

fn ensure_config_dir() -> std::io::Result<()> {
    // Will create .browsercfg and the nested browsers folder
    fs::create_dir_all(BROWSERS_FOLDER)
}

fn prompt_browser(option: &str) -> String {
    let browser_keys = BROWSER_MAP.keys().copied().collect::<Vec<_>>();

    let mut browser = "";
    let mut release_channel = "";

    if option.starts_with("--") && option != "--list" {
        let (browser, release_channel) = option.trim_start_matches("--").split_once('_').expect("expected format: --browser_channel");
        let release_channels = BROWSER_MAP[browser].keys().copied().collect::<Vec<_>>();
        if !browser_keys.contains(&browser) {
            panic!("err: unknown browser passed to prompt");
        } else if !release_channels.contains(&release_channel) {
            panic!("err: unknown release channel passed to prompt");
        }
    } else {
        browser = inquire::Select::new(
            "Select a browser:",
            browser_keys,
        )
        .prompt()
        .unwrap();

        let release_channels = BROWSER_MAP[browser].keys().collect::<Vec<_>>();
        release_channel = *release_channels.first().unwrap();
        if release_channels.len() > 1 {
            release_channel = inquire::Select::new("Select release channel:", release_channels)
                .prompt()
                .unwrap();
        } else {
            println!(
                "Only one valid release channel available, selected: {}",
                release_channels[0]
            );
        }
    }

    ensure_config_dir().expect("err: failed to define/verify config folder (.browsercfg)");
    let browser_string = format!("{} {}", browser, release_channel);
    fs::write(BROWSER_CONFIG, &browser_string).expect("err: failed to write browser config file");

    browser_string
}

fn get_os() -> String {
    if cfg!(windows) {
        return "windows".to_string();
    } else if cfg!(target_os = "linux") {
        return "linux".to_string();
    }

    panic!("err: unknown os found, but not supported");
}

fn download_binary(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut response = reqwest::blocking::get(url)?;
    let mut dest = fs::File::create(".browsercfg/tmp.bin")?;
    response.copy_to(&mut dest)?;
    Ok(())
}

fn extract(path: &str, out_dir: String) -> Result<(), Box<dyn std::error::Error>> {    
    #[cfg(windows)]
    {
        let data = fs::read(path).unwrap();
        // 131665 is the offset of the file, from which state everything is simply a 7z file
        let payload = std::io::Cursor::new(&data[131665..]);
        // Lie to 7z about the out_dir to prevent permissions issues when truely extracting
        sevenz_rust::decompress_with_extract_fn(payload, "", |entry, reader, _dest| {
            if let Some(relative) = entry.name().strip_prefix("core/") {
                let dest = std::path::PathBuf::from(&out_dir).join(relative);
                if entry.is_directory() {
                    fs::create_dir_all(&dest)?;
                } else {
                    if let Some(p) = dest.parent() { fs::create_dir_all(p)?; }
                    std::io::copy(reader, &mut fs::File::create(&dest)?)?;
                }
                return Ok(true);
            }
            Ok(false)
        }).unwrap();
    }

    #[cfg(unix)]
    {
        fs::create_dir_all(&out_dir);
        let file = fs::File::open(path)?;
        let xz = xz2::read::XzDecoder::new(file);
        let mut archive = tar::Archive::new(xz);
        let mut entries = archive.entries()?;
        entries.next(); // skip the {browser}/ folder entry
        for entry in entries {
            let mut entry = entry?;
            let path_buf = entry.path()?.to_path_buf();
            let path = path_buf.to_str().unwrap();
            let relative = &path[path.find('/').unwrap() + 1..];
            let dest = Path::new(&out_dir).join(relative);
            entry.unpack(&dest)?;
        }
    }

    Ok(())
}

fn download_browser(browser: String, release_channel: String) {
    println!("  Finding download url...");
    let browser_url = BROWSER_MAP[&*browser][&*release_channel][&*get_os()];

    println!("  Downloading browser...");
    download_binary(browser_url)
        .expect("err: failed to download browser binary, invalid internet connection?");

    let browser_path = format!("{BROWSERS_FOLDER}{browser}_{release_channel}");

    println!("  Extracting browser...");
    extract(
        ".browsercfg/tmp.bin",
        format!("{browser_path}/browser"),
    )
    .expect("err: cannot extract browser binary");

    fs::remove_file(".browsercfg/tmp.bin").expect("err: cannot remove browser binary");

    println!("  Configuring default settings...");
    let profile_path = format!("{browser_path}/profile");
    let _ = fs::create_dir_all(&profile_path);
    let _ = fs::write(format!("{profile_path}/prefs.js"), DEFAULT_PREFS);

    println!("  Successfully downloaded browser.");
}

fn get_browser(option: &str) -> (String, String) {
    let browser_str = match fs::read_to_string(BROWSER_CONFIG) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => select(option),
        Err(e) => {
            eprintln!("err: cannot parse browser config: {e}");
            std::process::exit(1);
        }
    };

    let (browser, release_channel) = browser_str.split_once(' ').unwrap_or(("", ""));
    (browser.to_string(), release_channel.to_string())
}

fn help() {
    println!("Help: browsercfg <action> <option?>");
    println!("  > help: Opens the current screen");
    println!("  > import: Imports code into curr_browser config");
    println!(
        "    > --{{curr_browser}}: Optional way to specify a curr_browser to import to temporarily"
    );
    println!("  > test: Runs tests on selected curr_browser");
    println!(
        "    > --{{curr_browser}}: Optional way to specify a curr_browser to test on temporarily"
    );
    println!("  > select: Interactively select a curr_browser to test on");
    println!("    > --get: Return selected curr_browser");
    println!("    > --list: If specified, will list supported browsers");
    println!("    > --{{curr_browser}}: Optional way to specify the curr_browser directly");
}

fn run(option: &str) {
    let (browser, release_channel) = get_browser(option);
    let browser_path = format!("{BROWSERS_FOLDER}{browser}_{release_channel}");

    let status = std::process::Command::new(format!(
        "{browser_path}/browser/{browser}{EXE_SUFFIX}"
    ))
    .arg("-profile")
    .arg(format!("{browser_path}/profile"))
    .status()
    .expect("err: failed to start browser");

    if !status.success() {
        panic!("process exited with: {}", status);
    }
}

fn format_import_path(path: &str, is_source: bool, base_browser_path: &str) -> String {
    // Destination should not be sanitized for special use cases
    let sanitized_path = secure::sanitize_str(path, is_source);
    let mut formatted_path = IMPORT_PATH_RE.replace(&sanitized_path, "").into_owned();
    
    // This function is highly repetitive. Likely a way to simplify it?
    if is_source {
        if sanitized_path.starts_with("{remote}") {
            let abs_remote_path = fs::canonicalize(remote::REMOTE_PATH).expect("err: cannot get absolute path for remote path");
            formatted_path = format!("{}/{formatted_path}", abs_remote_path.to_string_lossy());
        } else if path == "./" {
            formatted_path = "./".to_string();
        }
    } else {
        if sanitized_path.starts_with("{browser}") {
            let abs_browser_path = fs::canonicalize(
                format!("{base_browser_path}/browser")
            ).expect("err: cannot get absolute path for browser path");
            formatted_path = format!("{}/{formatted_path}", abs_browser_path.to_string_lossy());
        } else if sanitized_path.starts_with("{profile}") {
            let abs_profile_path = fs::canonicalize(
                format!("{base_browser_path}/profile")
            ).expect("err: cannot get absolute path for profile path");
            formatted_path = format!("{}/{formatted_path}", abs_profile_path.to_string_lossy());
        }
    }

    #[cfg(windows)]
    {
        formatted_path = formatted_path.replace('/', "\\");
    }

    formatted_path
}

fn import(option: &str) {
    let (browser, release_channel) = get_browser(option);
    let base_browser_path = format!("{BROWSERS_FOLDER}{browser}_{release_channel}");

    let config_data = match fs::read_to_string(CONFIG_FILE) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("err: config file must be declared for importing");
            return;
        }
    };
    let config: serde_json::Value = serde_json::from_str(&config_data).unwrap();

    if let Some(import_config) = &config.get("import").unwrap().as_object() {
        remote::check_status("import".to_string(), &config);
        
        let mut ignore: Vec<&str> = config["ignore"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_else(|| vec![]);
        ignore.extend([".browsercfg", ".git"]);

        for (source, dest) in import_config.iter() {
            let expanded_source = format_import_path(source, true, &base_browser_path);
            let dest_str = dest.as_str().expect("err: cannot represent destination path as &str");
            let expanded_dest = format_import_path(dest_str, false, &base_browser_path);
            copy_dir_all(expanded_source, expanded_dest, &ignore).expect("err: cannot copy");
        }
    }

    println!("\nSuccessfully imported the current project.");
}

fn test(_option: &str) {
    println!("Testing is not yet supported in this version. :(");
}

fn select(option: &str) -> String {
    let curr_browser_str =
        fs::read_to_string(BROWSER_CONFIG).unwrap_or_else(|_| "none".to_string());
    let (curr_browser, release_channel) = curr_browser_str.split_once(' ').unwrap_or(("", ""));

    if option == "--get" {
        println!("Current selected browser: {}", curr_browser_str);
        return "".to_string();
    } else if option == "--list" {
        println!("Supported browsers:");
        let browsers = BROWSER_MAP.keys().collect::<Vec<_>>();
        for browser in browsers {
            println!(
                "> {} {}",
                browser,
                if curr_browser == *browser {
                    "(curr)"
                } else {
                    ""
                }
            );
        }
        return "".to_string();
    }

    let browser_str = prompt_browser(option);

    // In this case {curr_browser} represents the previous browser
    if curr_browser_str != "none" {
        let browser_folder = format!("{BROWSERS_FOLDER}{curr_browser}_{release_channel}");
        if Path::new(&browser_folder).exists()
            && inquire::Confirm::new("Uninstall previous browser? (y/N)")
                .prompt()
                .unwrap()
        {
            fs::remove_dir_all(browser_folder)
                .expect("err: failed to remove browser folder recursively");
        }
    }

    if inquire::Confirm::new("Download the browser now? (y/N)")
        .prompt()
        .unwrap()
    {
        let (browser, release_channel) = browser_str.split_once(' ').unwrap();
        download_browser(browser.to_string(), release_channel.to_string());
    }

    browser_str
}

fn main() {
    if cfg!(target_os = "macos") {
        println!("As of v{CURR_VERSION} MacOS is not supported :(");
        println!("If necessary, you can try running browsercfg in a docker container or a VM.");
        std::process::exit(0);
    } else {
        // If browsercfg is running on an unsupported OS, this will panic
        get_os();
    }

    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let option = args.get(2).map(|s| s.as_str()).unwrap_or("");
        match args.get(1).map(|s| s.as_str()).unwrap_or("") {
            "help" => help(),
            "run" => run(option),
            "import" => import(option),
            "test" => test(option),
            "select" => {
                select(option);
            },
            _ => {
                println!("Unknown command!");
                help();
            }
        }
    }
}
