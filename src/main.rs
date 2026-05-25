use indexmap::IndexMap;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

mod remote;
mod secure;
mod tests;

#[cfg(windows)]
const EXE_SUFFIX: &str = ".exe";
#[cfg(unix)]
const EXE_SUFFIX: &str = "";

static IMPORT_PATH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^\{.+\}(/|$)").unwrap()
});

const CURR_VERSION: &str = env!("CARGO_PKG_VERSION");
const BROWSER_CONFIG: &str = ".browsercfg/curr_browser";
const BROWSERS_FOLDER: &str = ".browsercfg/browsers/";
const CONFIG_FILE: &str = ".browsercfg.json";

// Default prefs.js necessary to skip welcome and setup screens
const DEFAULT_PREFS: &str = "
  user_pref(\"zen.welcome-screen.seen\", true);
  user_pref(\"browser.shell.checkDefaultBrowser\", false);
";

// policies.json to disable updates for the isolated browser
const POLICIES: &str = "
  {
    \"policies\": {
      \"DisableAppUpdate\": true
    }
  }
";

const BOOTSTRAP_MJS: &str = include_str!("profile/bootstrap.sys.mjs");
const CHROME_MANIFEST: &str = include_str!("profile/chrome.manifest");
const CONFIG_JS: &str = include_str!("program/config.js");

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
                    (
                        "other",
                        HashMap::from([
                            (
                                "linux",
                                "https://download.mozilla.org/?product=firefox-*&os=linux64&lang=en-US",
                            ),
                            (
                                "windows",
                                "https://download.mozilla.org/?product=firefox-*&os=win64&lang=en-US",
                            ),
                        ]),
                    ),
                ]),
            ),
            (
                "floorp",
                IndexMap::from([
                    (
                        "stable",
                        HashMap::from([
                            (
                                "linux",
                                "https://github.com/Floorp-Projects/Floorp/releases/latest/download/floorp-linux-x86_64.tar.xz",
                            ),
                            (
                                "windows",
                                "https://github.com/Floorp-Projects/Floorp/releases/latest/download/floorp-windows-x86_64.installer.exe",
                            ),
                        ]),
                    ),
                    (
                        "other",
                        HashMap::from([
                            (
                                "linux",
                                "https://github.com/Floorp-Projects/Floorp/releases/download/v*/floorp-linux-x86_64.tar.xz",
                            ),
                            (
                                "windows",
                                "https://github.com/Floorp-Projects/Floorp/releases/download/v*/floorp-windows-x86_64.installer.exe",
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
                    (
                        "other",
                        HashMap::from([
                            (
                                "linux",
                                "https://github.com/zen-browser/desktop/releases/download/*/zen.linux-x86_64.tar.xz",
                            ),
                            (
                                "windows",
                                "https://github.com/zen-browser/desktop/releases/download/*/zen.installer.exe",
                            ),
                        ]),
                    ),
                ]),
            ),
        ])
    },
);

fn copy_all(src: impl AsRef<Path>, dst: impl AsRef<Path>, ignore: &Vec<&str>) -> std::io::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if src.is_file() {
        let dst = if dst.is_dir() {
            dst.join(src.file_name().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "source has no filename")
            })?)
        } else {
            dst.to_path_buf()
        };

        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, &dst)?;
    } else {
        fs::create_dir_all(&dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let file_name = entry.file_name();
            if !ignore.contains(&file_name.to_str().unwrap_or("")) {
                if ty.is_dir() { 
                    copy_all(entry.path(), dst.join(file_name), ignore)?;
                } else {
                    fs::copy(entry.path(), dst.join(file_name))?;
                }
            }
        }
    }
    
    Ok(())
}

fn ensure_config_dir() -> std::io::Result<()> {
    // Will create .browsercfg and the nested browsers folder
    fs::create_dir_all(BROWSERS_FOLDER)
}

fn prompt_browser(options: &Vec<&str>) -> String {
    let browser_keys = BROWSER_MAP.keys().copied().collect::<Vec<_>>();

    let (browser, release_channel) = if let Some(browser_name) = options.windows(2).find(|w| w[0] == "--browser").map(|w| w[1]) {
        let (browser, release_channel) = browser_name.split_once('-').expect("expected format: --browser browser-channel");
        let release_channels = BROWSER_MAP[browser].keys().copied().collect::<Vec<_>>();
        if !browser_keys.contains(&browser) {
            panic!("err: unknown browser passed to prompt");
        } else if !release_channels.contains(&release_channel) {
            panic!("err: unknown release channel passed to prompt");
        }
        (browser, release_channel.to_string())
    } else {
        let browser = inquire::Select::new(
            "Select a browser:",
            browser_keys,
        )
        .prompt()
        .unwrap();

        let release_channels = BROWSER_MAP[browser].keys().collect::<Vec<_>>();
        let mut release_channel = release_channels.first().copied().unwrap().to_string();
        if release_channels.len() > 1 {
            release_channel = inquire::Select::new("Select release channel:", release_channels)
                .prompt()
                .unwrap()
                .to_string();
        } else {
            println!(
                "Only one valid release channel available, selected: {}",
                release_channels[0]
            );
        }

        if release_channel == "other" {
            if let Some(version_tag) = options.windows(2).find(|w| w[0] == "--release").map(|w| w[1]) {                                                         release_channel = version_tag.to_string();
            } else {
                let version_tag = inquire::Text::new("Version to download (without the 'v' prefix):")
                    .prompt()
                    .unwrap();
                release_channel = format!("v{}", version_tag);
            }
        }

        (browser, release_channel)
    };

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

        const SIG_7Z: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
        let offset = data
            .windows(SIG_7Z.len())
            .position(|w| w == SIG_7Z)
            .expect("Could not find 7z signature in installer");

        let payload = std::io::Cursor::new(&data[offset..]);
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

    #[cfg(target_os = "linux")]
    {
        let _ = fs::create_dir_all(&out_dir);
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
    let query_channel = if release_channel.starts_with('v') {
        "other"
    } else {
        &release_channel
    };
    let version_tag = release_channel.strip_prefix('v').unwrap_or("");
    let browser_url = BROWSER_MAP[&*browser][&*query_channel][&*get_os()].replace('*', version_tag);

    println!("  Downloading browser...");
    download_binary(&browser_url)
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
    let _ = fs::create_dir_all(format!("{browser_path}/browser/distribution"));
    let _ = fs::write(format!("{browser_path}/browser/distribution/policies.json"), POLICIES);

    println!("  Successfully downloaded browser.");
}

fn get_browser(options: &Vec<&str>) -> (String, String) {
    let browser_str = match fs::read_to_string(BROWSER_CONFIG) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => select(options),
        Err(e) => {
            eprintln!("err: cannot parse browser config: {e}");
            std::process::exit(1);
        }
    };

    let (browser, release_channel) = browser_str.split_once(' ').unwrap_or(("", ""));
    (browser.to_string(), release_channel.to_string())
}

fn get_config() -> Option<serde_json::Value> {
    let config_data = match fs::read_to_string(CONFIG_FILE) {
        Ok(s) => s,
        Err(_) => return None,
    };
    serde_json::from_str(&config_data).expect("err: incorrect json format found")
}

fn help() {
    println!("Help: browsercfg <action> <option?>");
    println!("  > help: Opens the current screen");
    println!("  > import: Imports code into curr_browser config");
    println!("  > test: Runs tests on selected curr_browser");
    println!("  > select: Interactively select a curr_browser to test on");
    println!("    > --get: Return selected curr_browser");
    println!("    > --list: If specified, will list supported browsers");
    println!("    > --uninstall: Automatically say yes to uninstalling the previous browser");
    println!("    > --download: Automatically say yes to downloading the browser");
    println!("    > --browser {{curr_browser}}: Optional way to specify the curr_browser directly");
}

fn run(options: Vec<&str>) {
    let (browser, release_channel) = get_browser(&options);
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
    secure::assert_safe(path);
    let mut formatted_path = IMPORT_PATH_RE.replace(&path, "").into_owned();
    
    // This function is highly repetitive. Likely a way to simplify it?
    if is_source {
        if path.starts_with("{remote}") {
            let abs_remote_path = fs::canonicalize(remote::REMOTE_PATH).expect("err: cannot get absolute path for remote path");
            formatted_path = format!("{}/{formatted_path}", abs_remote_path.to_string_lossy());
        } else if path == "./" {
            formatted_path = "./".to_string();
        }
    } else {
        if path.starts_with("{browser}") {
            let abs_browser_path = fs::canonicalize(
                format!("{base_browser_path}/browser")
            ).expect("err: cannot get absolute path for browser path");
            formatted_path = format!("{}/{formatted_path}", abs_browser_path.to_string_lossy());
        } else if path.starts_with("{profile}") {
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

fn import(options: Vec<&str>) {
    let (browser, release_channel) = get_browser(&options);
    let base_browser_path = format!("{BROWSERS_FOLDER}{browser}_{release_channel}");

    let config = get_config().expect("err: config file must be declared to import");

    if let Some(import_config) = &config.get("import").unwrap().as_object() {
        remote::check_status("import".to_string(), &config);
        
        let mut ignore: Vec<&str> = config["ignore"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_else(|| vec![]);
        ignore.extend([".browsercfg", ".git", "node_modules"]);

        let import_scripts = &import_config.get("scripts").and_then(|v| v.as_object());
        if let Some(scripts) = import_scripts {
            if let Some(pre) = scripts.get("init") {
                secure::parse_script(pre, ".".to_string());
            }
        }

        let import_files = import_config.get("files").expect("err: import declared, but missing necessary files field")
            .as_object().expect("err: import field, files, is declared, but cannot be represented as an object");
        for (source, dest) in import_files.iter() {
            let expanded_source = format_import_path(source, true, &base_browser_path);
            let dest_str = dest.as_str().expect("err: cannot represent destination path as &str");
            let expanded_dest = format_import_path(dest_str, false, &base_browser_path);
            copy_all(expanded_source, expanded_dest, &ignore).expect("err: cannot copy");
        }

        if let Some(scripts) = import_scripts {
            if let Some(post) = scripts.get("cleanup") {
                secure::parse_script(post, ".".to_string());
            }
        }

        let chrome_dest = format!("{base_browser_path}/profile/chrome");
        fs::create_dir_all(&chrome_dest).expect("err: failed to create chrome folder");
        fs::write(format!("{chrome_dest}/bootstrap.sys.mjs"), BOOTSTRAP_MJS)
            .expect("err: failed to write bootstrap.sys.mjs");
        fs::write(format!("{chrome_dest}/chrome.manifest"), CHROME_MANIFEST)
            .expect("err: failed to write chrome.manifest");

        let config_js_dest = format!("{base_browser_path}/browser/config.js");
        let mut existing = fs::read_to_string(&config_js_dest).unwrap_or_default();
        if let Some(idx) = existing.find("// browsercfg:start") {
            existing.truncate(idx);
        }
        fs::write(&config_js_dest, format!(
            "{existing}\n// browsercfg:start\n{CONFIG_JS}\n// browsercfg:end\n"
        )).expect("err: failed to write config.js");

        println!("\nSuccessfully imported the current project.\n");
    } else {
        println!("No import configuration to import.\n");
    }
}

fn test(options: Vec<&str>) {
    let config = get_config().expect("err: config file must be declared to test");
    let test_config = match config.get("test") {
        Some(t) => t.clone(),
        None => {
            println!("No test configuration found.");
            return;
        }
    };

    let (browser, release_channel) = get_browser(&options);
    let base_browser_path = format!("{BROWSERS_FOLDER}{browser}_{release_channel}");

    if !options.contains(&"--no-import") {
        import(options);
    }

    let test_scripts = test_config.get("scripts");
    if let Some(scripts) = test_scripts {
        if let Some(init) = scripts.get("init") {
            secure::parse_script(init, ".".to_string());
        }
    }

    let results = tests::test(
        &format!("{base_browser_path}/profile"),
        &format!("{base_browser_path}/browser/{browser}{EXE_SUFFIX}"),
        &format!("{base_browser_path}/profile"),
    );
    // New line print
    println!();

    if let Some(scripts) = test_scripts {
        if let Some(cleanup) = scripts.get("cleanup") {
            secure::parse_script(cleanup, ".".to_string());
        }
    }

    if results.iter().any(|r| r.status == tests::TestStatus::Fail) {
        std::process::exit(1);
    }
}

fn select(options: &Vec<&str>) -> String {
    let curr_browser_str =
        fs::read_to_string(BROWSER_CONFIG).unwrap_or_else(|_| "none".to_string());
    let (curr_browser, release_channel) = curr_browser_str.split_once(' ').unwrap_or(("", ""));

    if options.contains(&"--get") {
        println!("Current selected browser: {}", curr_browser_str);
        return "".to_string();
    } else if options.contains(&"--list") {
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

    let browser_str = prompt_browser(options);

    // In this case {curr_browser} represents the previous browser
    if curr_browser_str != "none" {
        let browser_folder = format!("{BROWSERS_FOLDER}{curr_browser}_{release_channel}");
        if Path::new(&browser_folder).exists() &&
           (
               options.contains(&"--uninstall") ||
               inquire::Confirm::new("Uninstall previous browser? (y/N)")
                   .prompt()
                   .unwrap()
           )
        {
            fs::remove_dir_all(browser_folder)
                .expect("err: failed to remove browser folder recursively");
        }
    }

    if options.contains(&"--download") ||
       inquire::Confirm::new("Download the browser now? (y/N)")
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
        let options: Vec<&str> = args[2..].iter().map(|s| s.as_str()).collect();
        match args.get(1).map(|s| s.as_str()).unwrap_or("") {
            "help" => help(),
            "run" => run(options),
            "import" => import(options),
            "test" => test(options),
            "select" => {
                select(&options);
            },
            "--version" => {
                println!("Browsercfg is operational. Version: {CURR_VERSION} :)");
            },
            _ => {
                println!("Unknown command!");
                help();
            }
        }
    } else {
        help();
    }
}
