#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const { execSync, spawn } = require('child_process');

const isWindows = process.platform === 'win32';
const globalBinPath = execSync('npm config get prefix').toString().trim();

const binName = 'clg';
const binaryName = isWindows ? 'commitlg-cli-win-x64.exe' : 'commitlg-cli-linux-x64';
const binaryPath = path.join(__dirname, binaryName);

if (isWindows) {
  // --- WINDOWS: OVERRIDE 3 FILE WRAPPER ---
  const posixPath = binaryPath
    .replace(/^([a-zA-Z]):/, (_, drive) => `/${drive.toLowerCase()}`)
    .replace(/\\/g, '/');

  // 1. Override Bash wrapper (Git Bash)
  fs.writeFileSync(path.join(globalBinPath, binName), `#!/bin/sh\nexec "${posixPath}" "$@"`, { mode: 0o755 });

  // 2. Override CMD & BAT wrapper
  const cmdContent = `@echo off\r\n"${binaryPath}" %*`;
  fs.writeFileSync(path.join(globalBinPath, `${binName}.cmd`), cmdContent);

  // 3. Override PowerShell wrapper
  fs.writeFileSync(path.join(globalBinPath, `${binName}.ps1`), `& "${binaryPath}" $args`);

  spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
} else {
  // --- LINUX & MAC: SYMLINK NATIVE ---
  const targetSymlink = path.join(globalBinPath, 'bin', binName);

  // Pastikan binary asli punya akses eksekusi (+x)
  fs.chmodSync(binaryPath, '755');

  // Buat symlink langsung ke binary native
  if (fs.existsSync(targetSymlink)) fs.unlinkSync(targetSymlink);
  fs.symlinkSync(binaryPath, targetSymlink);

  spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
}
