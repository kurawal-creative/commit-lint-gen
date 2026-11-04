#!/usr/bin/env node
// postinstall: download the prebuilt binary for this platform from the GitHub release
// that matches this package version, and expose it as bin/clg.exe.
// The `.exe` suffix is intentional: the file is a native executable on every OS.
//
// Channel is derived from the packaged version: `x.y.z-beta.n` -> prerelease,
// otherwise -> latest stable. If the exact release is missing (e.g. installed
// from git, where package.json is stale), fall back to the newest release in
// that channel.
const fs = require('fs');
const path = require('path');
const { Readable } = require('stream');
const { pipeline } = require('stream/promises');

const REPO = 'kurawal-creative/commit-lint-gen';
const VERSION = require('../package.json').version;
const BINARIES = {
  'linux-x64': 'commitlg-cli-linux-x64',
  'win32-x64': 'commitlg-cli-win32-x64.exe',
  // macOS sementara dinonaktifkan.
  // 'darwin-x64': 'commitlg-cli-darwin-x64',
  // 'darwin-arm64': 'commitlg-cli-darwin-arm64',
};

function fail(message) {
  console.error(`commitlg: ${message}`);
  process.exit(1);
}

const key = `${process.platform}-${process.arch}`;
const binary = BINARIES[key];
if (!binary) {
  fail(`unsupported platform ${key}. Supported: ${Object.keys(BINARIES).join(', ')}`);
}

const binDir = path.join(__dirname, '..', 'bin');
const source = path.join(binDir, binary);
const target = path.join(binDir, 'clg.exe');

// Magic bytes: pastikan yang terunduh benar-benar executable, bukan halaman error.
const MAGIC = {
  linux: [[0x7f, 0x45, 0x4c, 0x46]],
  win32: [[0x4d, 0x5a]],
  darwin: [
    [0xcf, 0xfa, 0xed, 0xfe],
    [0xce, 0xfa, 0xed, 0xfe],
    [0xca, 0xfe, 0xba, 0xbe],
  ],
};

function isExpectedBinary(file) {
  const head = fs.readFileSync(file).subarray(0, 4);
  return (MAGIC[process.platform] || []).some((magic) =>
    magic.every((byte, i) => head[i] === byte),
  );
}

async function save(response, url) {
  if (!response.ok) {
    fail(`download failed (${response.status}): ${url}`);
  }
  fs.mkdirSync(binDir, { recursive: true });
  const tmp = `${target}.tmp`;
  await pipeline(Readable.fromWeb(response.body), fs.createWriteStream(tmp));
  if (!isExpectedBinary(tmp)) {
    fs.rmSync(tmp, { force: true });
    fail(`downloaded file is not a valid ${process.platform} executable: ${url}`);
  }
  fs.renameSync(tmp, target);
}

async function latestReleaseFor(channel) {
  const url = `https://api.github.com/repos/${REPO}/releases?per_page=30`;
  const response = await fetch(url, {
    headers: { 'User-Agent': `commitlg/${VERSION}`, Accept: 'application/vnd.github+json' },
  });
  if (!response.ok) {
    fail(`could not list releases (${response.status}): ${url}`);
  }
  const releases = await response.json();
  const release = releases.find((r) => !r.draft && (channel === 'beta' ? r.prerelease : !r.prerelease));
  if (!release) {
    fail(`no ${channel} release found in ${REPO}`);
  }
  return release;
}

async function main() {
  // Bundled binary wins: used by local builds and when release download is unavailable.
  if (fs.existsSync(source)) {
    fs.copyFileSync(source, target);
  } else {
    const channel = VERSION.includes('-') ? 'beta' : 'latest';
    const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${binary}`;
    let response = await fetch(url, {
      redirect: 'follow',
      headers: { 'User-Agent': `commitlg/${VERSION}` },
    });
    if (response.status === 404) {
      const release = await latestReleaseFor(channel);
      const asset = (release.assets || []).find((a) => a.name === binary);
      if (!asset) {
        fail(`release ${release.tag_name} has no asset ${binary}`);
      }
      response = await fetch(asset.browser_download_url, {
        redirect: 'follow',
        headers: { 'User-Agent': `commitlg/${VERSION}` },
      });
      await save(response, asset.browser_download_url);
    } else {
      await save(response, url);
    }
  }

  if (process.platform !== 'win32') {
    fs.chmodSync(target, 0o755);
  }

  console.log(`commitlg: installed ${key} -> bin/clg.exe`);
}

main().catch((error) => fail(error.message));
