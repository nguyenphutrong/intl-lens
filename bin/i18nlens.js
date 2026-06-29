#!/usr/bin/env node

const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");
const { spawn, spawnSync } = require("child_process");

const pkg = require("../package.json");

function platformTarget() {
  const platform = process.platform;
  const arch = process.arch;

  const mappedArch =
    arch === "x64" ? "x86_64" : arch === "arm64" ? "aarch64" : null;
  const mappedOs =
    platform === "linux"
      ? "unknown-linux-gnu"
      : platform === "darwin"
        ? "apple-darwin"
        : platform === "win32"
          ? "pc-windows-msvc"
          : null;

  if (!mappedArch || !mappedOs) {
    throw new Error(`Unsupported platform: ${platform}/${arch}`);
  }

  return { arch: mappedArch, os: mappedOs, isWindows: platform === "win32" };
}

function download(url, destination) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, (response) => {
      if (
        response.statusCode >= 300 &&
        response.statusCode < 400 &&
        response.headers.location
      ) {
        download(response.headers.location, destination).then(resolve, reject);
        return;
      }

      if (response.statusCode !== 200) {
        reject(new Error(`Download failed with HTTP ${response.statusCode}: ${url}`));
        return;
      }

      const file = fs.createWriteStream(destination);
      response.pipe(file);
      file.on("finish", () => file.close(resolve));
      file.on("error", reject);
    });

    request.on("error", reject);
  });
}

async function downloadFirst(candidates) {
  const errors = [];
  for (const candidate of candidates) {
    try {
      await download(candidate.url, candidate.destination);
      return candidate;
    } catch (error) {
      errors.push(error.message);
    }
  }

  throw new Error(errors.join("\n"));
}

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

async function ensureBinary() {
  const target = platformTarget();
  const version = process.env.I18NLENS_VERSION || `v${pkg.version}`;
  const cacheDir = path.join(os.homedir(), ".cache", "i18nlens", version);
  const binary = path.join(cacheDir, target.isWindows ? "i18nlens.exe" : "i18nlens");
  const legacyBinary = path.join(
    cacheDir,
    target.isWindows ? "intl-lens.exe" : "intl-lens",
  );

  if (fs.existsSync(binary)) {
    return binary;
  }
  if (fs.existsSync(legacyBinary)) {
    return legacyBinary;
  }

  fs.mkdirSync(cacheDir, { recursive: true });
  const extension = target.isWindows ? "zip" : "tar.gz";
  const asset = `i18nlens-${target.arch}-${target.os}.${extension}`;
  const legacyAsset = `intl-lens-${target.arch}-${target.os}.${extension}`;
  const url = `https://github.com/nguyenphutrong/i18nlens/releases/download/${version}/${asset}`;
  const legacyUrl = `https://github.com/nguyenphutrong/i18nlens/releases/download/${version}/${legacyAsset}`;
  const archive = path.join(cacheDir, asset);
  const legacyArchive = path.join(cacheDir, legacyAsset);

  console.error(`Downloading i18nlens ${version} for ${target.arch}-${target.os}...`);
  const downloaded = await downloadFirst([
    { url, destination: archive, binary },
    {
      url: legacyUrl,
      destination: legacyArchive,
      binary: legacyBinary,
    },
  ]);

  if (target.isWindows) {
    run("powershell", [
      "-NoProfile",
      "-Command",
      `Expand-Archive -Force '${downloaded.destination}' '${cacheDir}'`,
    ]);
  } else {
    run("tar", ["-xzf", downloaded.destination, "-C", cacheDir]);
    fs.chmodSync(downloaded.binary, 0o755);
  }

  return downloaded.binary;
}

ensureBinary()
  .then((binary) => {
    const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });
    child.on("exit", (code, signal) => {
      if (signal) {
        process.kill(process.pid, signal);
      }
      process.exit(code ?? 0);
    });
    child.on("error", (error) => {
      console.error(error.message);
      process.exit(1);
    });
  })
  .catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
