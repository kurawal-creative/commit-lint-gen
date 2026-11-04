#!/usr/bin/env node
// postinstall: pick the prebuilt binary for this platform and expose it as bin/clg.exe.
// The `.exe` suffix is intentional: the file is a native executable on every OS.
const fs = require('fs');
const path = require('path');

const BINARIES = {
  'linux-x64': 'commitlg-cli-linux-x64',
  'win32-x64': 'commitlg-cli-win-x64.exe',
  'darwin-x64': 'commitlg-cli-darwin-x64',
  'darwin-arm64': 'commitlg-cli-darwin-arm64',
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

if (!fs.existsSync(source)) {
  fail(`missing prebuilt binary ${binary}. Reinstall with: npm i -g commitlg`);
}

fs.copyFileSync(source, target);
if (process.platform !== 'win32') {
  fs.chmodSync(target, 0o755);
}

console.log(`commitlg: installed ${key} -> bin/clg.exe`);
