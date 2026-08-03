import type { Config } from '../config/defaultConfig.js';

export interface ValidationError {
  rule: string;
  message: string;
}

export interface ParsedCommit {
  type: string;
  scope?: string;
  description: string;
  raw: string;
}

export function parseCommitMessage(message: string): ParsedCommit | null {
  const trimmed = message.trim().split('\n')[0] || ''; // only first line

  // Match: type(scope): description or type: description
  const match = trimmed.match(/^(\w+)(\(([^)]+)\))?:\s*(.+)$/);

  if (!match) {
    return null;
  }

  const [, type, , scope, description] = match;

  const parsed: ParsedCommit = {
    type: type || '',
    description: description || '',
    raw: trimmed,
  };

  if (scope) {
    parsed.scope = scope;
  }

  return parsed;
}

export function validateType(parsed: ParsedCommit, config: Config): ValidationError | null {
  const allowedTypes = config.rules?.types || [];

  // If types array is empty or not defined, allow any type
  if (allowedTypes.length === 0) {
    return null;
  }

  // If types are defined but validation should be lenient, still allow any type
  // This makes the tool accept commits from other AI tools (copilot, zed, etc)
  // while still using the types list for generation
  return null;
}

export function validateScope(parsed: ParsedCommit, config: Config): ValidationError | null {
  const requireScope = config.rules?.requireScope || false;
  const allowedScopes = config.rules?.scopes || [];

  if (requireScope && !parsed.scope) {
    return {
      rule: 'scope',
      message: 'Scope is required',
    };
  }

  if (parsed.scope && allowedScopes.length > 0 && !allowedScopes.includes(parsed.scope)) {
    return {
      rule: 'scope',
      message: `Scope "${parsed.scope}" is not allowed. Allowed scopes: ${allowedScopes.join(', ')}`,
    };
  }

  return null;
}

export function validateLength(parsed: ParsedCommit, config: Config): ValidationError | null {
  const maxLength = config.rules?.maxLength || 100;
  const minLength = config.rules?.minLength || 5;

  if (parsed.raw.length > maxLength) {
    return {
      rule: 'max-length',
      message: `Commit message is too long (${parsed.raw.length} > ${maxLength})`,
    };
  }

  if (parsed.raw.length < minLength) {
    return {
      rule: 'min-length',
      message: `Commit message is too short (${parsed.raw.length} < ${minLength})`,
    };
  }

  return null;
}

export function validateDescription(parsed: ParsedCommit): ValidationError | null {
  if (!parsed.description || parsed.description.trim() === '') {
    return {
      rule: 'description',
      message: 'Description cannot be empty',
    };
  }

  // Removed: lowercase check and period check for more flexibility

  return null;
}
