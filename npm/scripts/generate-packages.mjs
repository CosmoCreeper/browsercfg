import * as fs from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { format } from "node:util";

const CLI_ROOT = resolve(fileURLToPath(import.meta.url), "../..");
const REPO_ROOT = resolve(CLI_ROOT, "..");
const MANIFEST_PATH = resolve(CLI_ROOT, "package.json");

const rootManifest = JSON.parse(fs.readFileSync(MANIFEST_PATH, "utf-8"));

const version = fs.readFileSync(resolve(REPO_ROOT, "Cargo.toml"), "utf-8")
  .match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];

// process.argv[2] will be tag name, and process.argv[3] will be a force argument, if it exists.
if (version !== process.argv[2].slice(1) && process.argv[3] !== "-f") {
  console.error("Mismatching tag name and Cargo.toml version, you may specify -f to force the update.");
  process.exit(1);
}

rootManifest.version = version;
rootManifest.optionalDependencies = {};

fs.copyFileSync(resolve(REPO_ROOT, "README.md"), resolve(CLI_ROOT, "README.md"));

const { license, repository, engines, homepage } = rootManifest;

function getName(platform, arch, prefix = "browsercfg") {
  return format(`${prefix}-${platform}`, arch);
}

function copyBinaryToNativePackage(platform, arch) {
  const os = platform.split("-")[0];
  const buildName = getName(platform, arch);
  const packageRoot = resolve(REPO_ROOT, buildName);
  fs.mkdirSync(packageRoot);
  const packageName = `@chromejs/${buildName}`;

  // Update the package.json manifest
  rootManifest.optionalDependencies[packageName] = version;

  const manifest = JSON.stringify(
    {
      name: packageName,
      version,
      license,
      repository,
      engines,
      homepage,
      os: [os],
      cpu: [arch],
      libc:
        os === "linux"
          ? packageName.endsWith("musl")
		    ? ["musl"]
		    : ["glibc"]
		: undefined,
    },
    null,
    2,
  );

  const manifestPath = resolve(packageRoot, "package.json");
  console.info(`Update manifest ${manifestPath}`);
  fs.writeFileSync(manifestPath, manifest);

  // Copy the CLI binary
  const ext = os === "win32" ? ".exe" : "";
  const binarySource = resolve(
    REPO_ROOT,
    `${getName(platform, arch)}${ext}`,
  );
  const binaryTarget = resolve(packageRoot, `browsercfg${ext}`);

  if (!fs.existsSync(binarySource)) {
    console.error(
      `Source for binary for ${buildName} not found at: ${binarySource}`,
    );
    process.exit(1);
  }

  console.info(`Copy binary ${binaryTarget}`);
  fs.copyFileSync(binarySource, binaryTarget);
  fs.chmodSync(binaryTarget, 0o755);
}

const PLATFORMS = ["win32-%s", "darwin-%s", "linux-%s", "linux-%s-musl"];
const ARCHITECTURES = ["x64", "arm64"];

for (const platform of PLATFORMS) {
  for (const arch of ARCHITECTURES) {
    copyBinaryToNativePackage(platform, arch);
  }
}

fs.writeFileSync(MANIFEST_PATH, JSON.stringify(rootManifest, null, 2), "utf-8");
