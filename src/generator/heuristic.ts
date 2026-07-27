import type { SimpleGit } from 'simple-git';

export interface HeuristicResult {
  type: string;
  scope?: string | undefined;
  description: string;
  body?: string;
  confidence: 'low' | 'medium' | 'high';
}

interface FileChange {
  path: string;
  additions: number;
  deletions: number;
  status: 'added' | 'modified' | 'deleted' | 'renamed';
}

const TYPE_PATTERNS = [
  { pattern: /^test[s]?\/|\.test\.|\.spec\.|__tests__/, type: 'test' },
  { pattern: /^docs?\/|\.md$|README/, type: 'docs' },
  { pattern: /package\.json|package-lock\.json|pnpm-lock\.yaml|yarn\.lock/, type: 'chore' },
  { pattern: /\.(css|scss|sass|less|styl)$/, type: 'style' },
  { pattern: /^\.github\/|\.gitlab-ci|\.circleci|Dockerfile|docker-compose/, type: 'ci' },
  { pattern: /eslint|prettier|tsconfig|\.config\.(js|ts)/, type: 'chore' },
];

const SCOPE_PATTERNS = [
  { pattern: /^(?:src\/)?components?\//i, scope: 'components' },
  { pattern: /^(?:src\/)?utils?\//i, scope: 'utils' },
  { pattern: /^(?:src\/)?api\//i, scope: 'api' },
  { pattern: /^(?:src\/)?hooks?\//i, scope: 'hooks' },
  { pattern: /^(?:src\/)?services?\//i, scope: 'services' },
  { pattern: /^(?:src\/)?models?\//i, scope: 'models' },
  { pattern: /^(?:src\/)?types?\//i, scope: 'types' },
  { pattern: /^(?:src\/)?lib\//i, scope: 'lib' },
  { pattern: /^(test[s]?|__tests__)\//i, scope: 'tests' },
  { pattern: /^docs?\//i, scope: 'docs' },
];

/**
 * Git shows renamed file in two format:
 *   - "old.ts => new.ts"
 *   - "src/{old => new}/file.ts"
 * This function detect both and return final path.
 */
function extractRenamedPath(rawPath: string): { path: string; isRename: boolean } {
  // Format brace: "src/{old => new}/file.ts"
  const braceMatch = rawPath.match(/^(.*)\{.*=>\s*(.*?)\}(.*)$/);
  if (braceMatch) {
    const [, prefix, newSegment, suffix] = braceMatch;
    const newPath = `${prefix ?? ''}${newSegment ?? ''}${suffix ?? ''}`.replace(/\/{2,}/g, '/');
    return { path: newPath.trim(), isRename: true };
  }

  // Format simple: "old.ts => new.ts"
  const simpleMatch = rawPath.match(/^(.+?)\s*=>\s*(.+)$/);
  if (simpleMatch) {
    const [, , newPath] = simpleMatch;
    if (newPath) {
      return { path: newPath.trim(), isRename: true };
    }
  }

  return { path: rawPath, isRename: false };
}

function parseFileChanges(diffSummary: string): FileChange[] {
  const changes: FileChange[] = [];
  const lines = diffSummary.split('\n').filter((line) => line.trim());

  for (const line of lines) {
    // Format binary file: " assets/logo.png | Bin 12345 -> 67890 bytes"
    const binaryMatch = line.match(/^\s*(.+?)\s+\|\s+Bin\s+\d+\s*->\s*\d+\s+bytes/);
    if (binaryMatch) {
      const rawPath = binaryMatch[1];
      if (rawPath) {
        const { path, isRename } = extractRenamedPath(rawPath.trim());
        changes.push({
          path,
          additions: 1,
          deletions: 0,
          status: isRename ? 'renamed' : 'modified',
        });
      }
      continue;
    }

    // Format git diff --stat: "path | 10 +++++-----"
    let match = line.match(/^\s*(.+?)\s+\|\s+(\d+)\s*([+-]+)?/);

    // Fallback to format simple numstat: "10\t5\tpath"
    if (!match) {
      match = line.match(/^(\d+)\s+(\d+)\s+(.+)$/);
      if (match) {
        const [, add, del, rawPath] = match;
        if (!add || !del || !rawPath) continue;

        const { path, isRename } = extractRenamedPath(rawPath.trim());
        const additions = parseInt(add, 10);
        const deletions = parseInt(del, 10);
        let status: FileChange['status'] = isRename ? 'renamed' : 'modified';

        if (!isRename && additions > 0 && deletions === 0) {
          status = 'added';
        } else if (!isRename && additions === 0 && deletions > 0) {
          status = 'deleted';
        }

        changes.push({ path, additions, deletions, status });
        continue;
      }
      continue;
    }

    const [, rawPath, changeCount] = match;
    if (!rawPath || !changeCount) continue;

    const { path, isRename } = extractRenamedPath(rawPath.trim());
    const totalChanges = parseInt(changeCount, 10);

    let additions = 0;
    let deletions = 0;
    let status: FileChange['status'] = isRename ? 'renamed' : 'modified';

    const plusCount = (match[3] || '').split('+').length - 1;
    const minusCount = (match[3] || '').split('-').length - 1;

    if (!isRename && plusCount > 0 && minusCount === 0) {
      additions = totalChanges;
      status = 'added';
    } else if (!isRename && plusCount === 0 && minusCount > 0) {
      deletions = totalChanges;
      status = 'deleted';
    } else {
      additions = Math.ceil(totalChanges / 2);
      deletions = Math.floor(totalChanges / 2);
    }

    changes.push({ path, additions, deletions, status });
  }

  return changes;
}

function inferType(changes: FileChange[]): string {
  const votes: Record<string, number> = {};

  for (const change of changes) {
    for (const { pattern, type } of TYPE_PATTERNS) {
      if (pattern.test(change.path)) {
        votes[type] = (votes[type] || 0) + 1;
      }
    }

    if (change.status === 'added') {
      votes['feat'] = (votes['feat'] || 0) + 1;
    } else if (change.status === 'deleted') {
      votes['refactor'] = (votes['refactor'] || 0) + 1;
    } else if (change.status === 'renamed') {
      votes['refactor'] = (votes['refactor'] || 0) + 1;
    }
  }

  if (Object.keys(votes).length === 0) {
    const totalAdditions = changes.reduce((sum, c) => sum + c.additions, 0);
    const totalDeletions = changes.reduce((sum, c) => sum + c.deletions, 0);

    if (totalAdditions > totalDeletions * 2) {
      return 'feat';
    } else if (totalDeletions > totalAdditions) {
      return 'refactor';
    }
    return 'fix';
  }

  return Object.entries(votes).sort((a, b) => b[1] - a[1])[0]![0]!;
}

function inferScope(changes: FileChange[]): string | undefined {
  const votes: Record<string, number> = {};

  for (const change of changes) {
    for (const { pattern, scope } of SCOPE_PATTERNS) {
      if (pattern.test(change.path)) {
        votes[scope] = (votes[scope] || 0) + 1;
      }
    }
  }

  if (Object.keys(votes).length === 0) return undefined;

  const topScope = Object.entries(votes).sort((a, b) => b[1] - a[1])[0];
  if (!topScope) return undefined;

  const totalVotes = Object.values(votes).reduce((sum, v) => sum + v, 0);
  return topScope[1] / totalVotes > 0.6 ? topScope[0] : undefined;
}

function generateDescription(type: string, changes: FileChange[], scope?: string): string {
  const added = changes.filter((c) => c.status === 'added').length;
  const deleted = changes.filter((c) => c.status === 'deleted').length;
  const modified = changes.filter((c) => c.status === 'modified').length;
  const renamed = changes.filter((c) => c.status === 'renamed').length;

  if (type === 'feat') {
    if (scope) return `add new ${scope} functionality`;
    if (added > 0) return `add new features`;
    return `implement new functionality`;
  }

  if (type === 'fix') {
    if (scope) return `resolve issues in ${scope}`;
    return `resolve bugs and issues`;
  }

  if (type === 'docs') {
    return `update documentation`;
  }

  if (type === 'test') {
    if (added > 0) return `add test coverage`;
    return `update tests`;
  }

  if (type === 'refactor') {
    if (renamed > 0 && renamed >= deleted && renamed >= modified) {
      return scope ? `rename and reorganize ${scope} files` : `rename and reorganize files`;
    }
    if (deleted > modified) return `remove deprecated code`;
    if (scope) return `refactor ${scope} implementation`;
    return `refactor code structure`;
  }

  if (type === 'style') {
    return `update styles and formatting`;
  }

  if (type === 'chore') {
    if (changes.some((c) => c.path.includes('package'))) {
      return `update dependencies`;
    }
    return `update build configuration`;
  }

  if (type === 'ci') {
    return `update CI/CD pipeline`;
  }

  if (scope) return `update ${scope}`;
  return `update codebase`;
}

function calculateConfidence(changes: FileChange[], type: string, scope?: string): 'low' | 'medium' | 'high' {
  if (changes.length === 0) return 'low';
  if (changes.length === 1) return 'high';

  const hasMatchingPattern = changes.some((c) =>
    TYPE_PATTERNS.some((p) => p.type === type && p.pattern.test(c.path))
  );

  if (hasMatchingPattern && scope) return 'high';
  if (hasMatchingPattern || scope) return 'medium';
  return 'low';
}

export async function generateHeuristicCommit(git: SimpleGit): Promise<HeuristicResult> {
  const isRepo = await git.checkIsRepo();
  if (!isRepo) {
    throw new Error('Not a git repository. Run this command from the root of a git repository.');
  }

  const diffSummary = await git.diff(['--stat', '--cached']);

  if (!diffSummary || diffSummary.trim() === '') {
    return {
      type: 'chore',
      description: 'update files',
      confidence: 'low',
    };
  }

  const changes = parseFileChanges(diffSummary);
  const type = inferType(changes);
  const scope = inferScope(changes);
  const description = generateDescription(type, changes, scope);
  const confidence = calculateConfidence(changes, type, scope);

  return { type, scope, description, confidence };
}