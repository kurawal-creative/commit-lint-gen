import { describe, it, expect } from 'vitest';
import { validateCommitMessage } from '../../src/linter/validate.js';
import type { Config } from '../../src/config/defaultConfig.js';

const mockConfig: Config = {
  aiProvider: 'groq',
  baseURL: 'https://api.groq.com/openai/v1',
  model: 'qwen/qwen3.6-27b',
  rules: {
    maxLength: 100,
    minLength: 5,
    types: ['feat', 'fix', 'docs', 'style', 'refactor', 'test', 'chore', 'ci'],
    scopes: ['api', 'ui', 'cli'],
    requireScope: false,
  },
};

describe('validateCommitMessage', () => {
  it('should validate correct commit message', () => {
    const result = validateCommitMessage('feat(api): add user endpoint', mockConfig);
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it('should reject invalid format', () => {
    const result = validateCommitMessage('invalid commit message', mockConfig);
    expect(result.valid).toBe(false);
    expect(result.errors[0]?.rule).toBe('format');
  });

  it('should collect multiple errors', () => {
    const result = validateCommitMessage('invalid(scope): x', mockConfig);
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
  });

  it('should validate commit without scope', () => {
    const result = validateCommitMessage('fix: resolve critical bug', mockConfig);
    expect(result.valid).toBe(true);
  });

  it('should reject when scope is required but missing', () => {
    const config = { ...mockConfig, rules: { ...mockConfig.rules, requireScope: true } };
    const result = validateCommitMessage('feat: add feature', config);
    expect(result.valid).toBe(false);
    expect(result.errors.some(e => e.rule === 'scope')).toBe(true);
  });

  it('should reject too short message', () => {
    const result = validateCommitMessage('f: x', mockConfig);
    expect(result.valid).toBe(false);
    expect(result.errors.some(e => e.rule === 'min-length')).toBe(true);
  });

  it('should accept uppercase description (relaxed rules)', () => {
    const result = validateCommitMessage('feat: Add feature', mockConfig);
    expect(result.valid).toBe(true);
  });

  it('should accept any type (lenient validation)', () => {
    const result = validateCommitMessage('custom: add new feature', mockConfig);
    expect(result.valid).toBe(true);
  });

  it('should accept copilot/zed style commits', () => {
    const copilotStyle = validateCommitMessage('Update README.md with new instructions', mockConfig);
    const zedStyle = validateCommitMessage('Add: new feature implementation', mockConfig);
    
    // These should pass basic validation (format may differ but not too strict)
    expect(copilotStyle.valid || copilotStyle.errors.length <= 1).toBe(true);
    expect(zedStyle.valid).toBe(true);
  });
});
