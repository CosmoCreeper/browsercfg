# browsercfg

A tool intended to help managing Firefox-based AutoConfig projects.
This tool allows you to locally install isolated browsers and import data into it.
This removes the constant tinkering and breaking of your browser, easily.

## Installation

### NPM

Just run this:
```
npm install --save-dev browsercfg
```
Now you can start using browsercfg like this:
```
npx browsercfg --help
```

### Cargo

Just run this:
```
cargo install browsercfg
```
Now you can start using browsercfg like this:
```
browsercfg --help
```

## Configuring browsercfg

To support many projects, browsercfg has a .browsercfg.json file that you can define.
Here's what an example one might look like:
```
{
    "remote": {
        "run_on": ["import"],
        "init": [
            ["git_clone", "https://github.com/sineorg/bootloader.git", "bootloader"]
            ["cd", "bootloader"],
            ["git_checkout", "-b", "beta", "origin/beta"]
        ],
        "update": [
            ["cd", "bootloader"],
            ["git_pull", "origin", "beta"]
        ]
    },
    "import": {
        "scripts": {
            "init": [
                ["py_exec", "scripts/package.py"]
            ],
            "cleanup": []
        },
        "files": {
            "{remote}/bootloader/program": "{browser}",
            "{remote}/bootloader/profile/utils": "{profile}/chrome/utils",
            "src": "{profile}/chrome/JS",
            "locales": "{profile}/chrome/JS/locales"
        }
    }
}
```
At first this might seem weird and hard to understand, but it's actually quite simple.

**This config file first sets up a remote config:**

This config exists because some projects rely on importing web resources from a repository.
This remote config will clone the sineorg/bootloader repository from GitHub to .browsercfg/remote/bootloader,
then it will checkout the origin/beta branch.
However, this only happens when it needs to initialize the remote config.
Future calls are just updates that will pull the latest data for that repository.
The remote config commands are sandboxed to only the ones you see in this example. (including the py_exec one from the import scripts)

**Next up is the import config:**

The *scripts* in the import config work very similar to the remote config, except this is tailored more towards the current repository
rather than the remote repositories. The init script will run immediately before importing (after the remote config), and the cleanup
script will run immediately after importing.
The current init script for the example shown above will execute a python script (via the python3 alias) in the `./scripts/package.py` path.
The current cleanup script is empty, and does nothing.

In the *files* property of the import config, every key represents a local path
and every value represents where you want to copy that file/folder (recursively, if needed) to.
Keys have access to {remote} which represents the .browsercfg/remote folder,
while values have access to {browser}, which represents the .browsercfg/browsers/{browser}/browser folder,
and {profile}, which represents the .browsercfg/browsers/{browser}/profile folder.
To give an example, the src key in the import config imports the src folder to the profile's chrome/JS folder,
creating all the necessary directories if missing.

## Why Rust?

I took this project as an opportunity to learn some Rust and build a low-level program.
This way I have learned some Rust and the project has much more of an opportunity to be fast than it would as an NPM package.

## Relevant links

https://github.com/CosmoCreeper/browsercfg \
https://crates.io/crates/browsercfg \
https://npmjs.com/package/@chromejs/browsercfg

