import { extname } from 'node:path';

export interface DiffProcessorConfig {
  maxTotalChars?: number;
  ignoredExts?: string[];
  ignoreLocks?: boolean;
}

const DEFAULT_IGNORED_EXTS = [
  '.png', '.jpg', '.jpeg', '.gif', '.svg', '.ico',
  '.pdf', '.zip', '.tar', '.gz', '.wasm', '.mp4'
];

const LOCKFILE_REGEX = /(package-lock\.json|pnpm-lock\.yaml|yarn\.lock|Cargo\.lock|go\.sum)$/i;
const HUNK_HEADER_REGEX = /^@@\s+-\d+(?:,\d+)?\s+\+\d+(?:,\d+)?\s+@@(.*)/;

export function processGitDiff(rawDiff: string, config: DiffProcessorConfig = {}): string {
  if (!rawDiff || !rawDiff.trim()) {
    return '';
  }

  const {
    maxTotalChars = 8000,
    ignoredExts = DEFAULT_IGNORED_EXTS,
    ignoreLocks = true,
  } = config;

  const rawFiles = rawDiff.split('diff --git ');
  const processedFiles: string[] = [];
  let totalChars = 0;
  let omittedFiles = 0;

  for (const rawFile of rawFiles) {
    if (!rawFile.trim()) continue;

    const lines = rawFile.split('\n');
    const headerLine = lines[0] ?? '';
    const filePath = extractFilePath(headerLine);

    if (shouldSkipFile(filePath, ignoredExts, ignoreLocks)) {
      omittedFiles++;
      continue;
    }

    const cleanedLines: string[] = [];
    cleanedLines.push(`FILE: ${filePath}`);
    
    let addedLines = 0;
    let removedLines = 0;

    for (let i = 1; i < lines.length; i++) {
      const line = lines[i]!;

      if (
        line.startsWith('index ') ||
        line.startsWith('--- ') ||
        line.startsWith('+++ ')
      ) {
        continue;
      }

      const trimmedLine = line.trimEnd();

      if (trimmedLine.startsWith('@@')) {
        const match = HUNK_HEADER_REGEX.exec(trimmedLine);
        if (match) {
          const funcContext = match[1]?.trim();
          if (funcContext) {
            cleanedLines.push(`@@ ${funcContext} @@`);
          } else {
            cleanedLines.push('@@');
          }
          continue;
        }
      }

      if (trimmedLine === '+' || trimmedLine === '-' || trimmedLine === '') {
        continue;
      }

      // Count added/removed lines
      if (trimmedLine.startsWith('+')) addedLines++;
      if (trimmedLine.startsWith('-')) removedLines++;

      cleanedLines.push(trimmedLine);
    }

    // Add stats summary for better context
    const statsLine = `[+${addedLines} -${removedLines}]`;
    cleanedLines.splice(1, 0, statsLine);

    const fileContent = cleanedLines.join('\n');

    if (totalChars + fileContent.length > maxTotalChars) {
      omittedFiles++;
      continue;
    }

    totalChars += fileContent.length;
    processedFiles.push(fileContent);
  }

  let result = processedFiles.join('\n\n');

  if (omittedFiles > 0) {
    result += `\n\n[${omittedFiles} file(s) omitted to fit token budget]`;
  }

  return result;
}

function extractFilePath(headerLine: string): string {
  const parts = headerLine.trim().split(/\s+/);
  if (parts.length >= 2) {
    return parts[1]!.replace(/^b\//, '');
  }
  return '';
}

function shouldSkipFile(filePath: string, ignoredExts: string[], ignoreLocks: boolean): boolean {
  if (!filePath) return false;

  if (ignoreLocks && LOCKFILE_REGEX.test(filePath)) {
    return true;
  }

  const ext = extname(filePath).toLowerCase();
  if (ignoredExts.includes(ext)) {
    return true;
  }

  return false;
}
