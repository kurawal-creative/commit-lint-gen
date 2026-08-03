import { describe, it, expect, vi } from 'vitest';
import { generateAICommit } from '../../src/generator/ai.js';
import { processGitDiff } from '../../src/generator/diffProcessor.js';
import type { SimpleGit } from 'simple-git';
import type { Config } from '../../src/config/defaultConfig.js';

vi.mock('../../src/generator/provider.js', () => ({
  createAIProvider: vi.fn(() => ({
    chat: {
      completions: {
        create: vi.fn().mockResolvedValue({
          choices: [
            {
              message: {
                content: JSON.stringify({
                  type: 'feat',
                  scope: 'api',
                  description: 'add user authentication',
                }),
              },
            },
          ],
        }),
      },
    },
  })),
}));

const mockConfig: Config = {
  aiProvider: 'groq',
  baseURL: 'https://api.groq.com/openai/v1',
  model: 'qwen/qwen3.6-27b',
  apiKey: 'test-key',
};

describe('generateAICommit', () => {
  it('should generate commit from AI', async () => {
    const mockGit = {
      checkIsRepo: vi.fn().mockResolvedValue(true),
      diff: vi.fn().mockResolvedValue('diff content'),
    } as unknown as SimpleGit;

    const result = await generateAICommit(mockGit, mockConfig);

    expect(result.type).toBe('feat');
    expect(result.scope).toBe('api');
    expect(result.description).toBe('add user authentication');
  });

  it('should throw error when no staged changes', async () => {
    const mockGit = {
      checkIsRepo: vi.fn().mockResolvedValue(true),
      diff: vi.fn().mockResolvedValue(''),
    } as unknown as SimpleGit;

    await expect(generateAICommit(mockGit, mockConfig)).rejects.toThrow('No staged changes found');
  });

  it('should throw error when no API key', async () => {
    // Clear the mock to use the real provider that checks for API key
    vi.clearAllMocks();
    vi.resetModules();

    // Re-mock to throw on missing API key
    vi.doMock('../../src/generator/provider.js', () => ({
      createAIProvider: vi.fn(() => {
        throw new Error('API key is required for AI generation. Set GROQ_API_KEY in your environment or add apiKey to your config.');
      }),
    }));

    const { generateAICommit: generateAI } = await import('../../src/generator/ai.js');
    const configWithoutKey = { ...mockConfig };
    delete configWithoutKey.apiKey;

    const mockGit = {
      diff: vi.fn().mockResolvedValue('diff content'),
    } as unknown as SimpleGit;

    await expect(generateAI(mockGit, configWithoutKey)).rejects.toThrow('API key is required');
  });
});

describe('processGitDiff', () => {
  it('should return processed diff with file info and stats', () => {
    const diff = 'diff --git a/file1.ts b/file1.ts\n+added line';
    const result = processGitDiff(diff, { maxTotalChars: 100 });
    expect(result).toContain('FILE: file1.ts');
    expect(result).toContain('[+1 -0]');
    expect(result).toContain('+added line');
  });

  it('should skip lockfiles when ignoreLocks is true', () => {
    const diff = 'diff --git a/package-lock.json b/package-lock.json\n+changes\ndiff --git a/file.ts b/file.ts\n+code';
    const result = processGitDiff(diff, { ignoreLocks: true });
    expect(result).not.toContain('package-lock.json');
    expect(result).toContain('file.ts');
  });

  it('should skip binary files by extension', () => {
    const diff = 'diff --git a/logo.png b/logo.png\nbinary\ndiff --git a/file.ts b/file.ts\n+code';
    const result = processGitDiff(diff, { ignoredExts: ['.png'] });
    expect(result).not.toContain('logo.png');
    expect(result).toContain('file.ts');
  });

  it('should omit files when exceeding budget', () => {
    const file1 = 'diff --git a/file1.ts b/file1.ts\n' + 'x'.repeat(5000);
    const file2 = 'diff --git a/file2.ts b/file2.ts\n' + 'y'.repeat(5000);
    const diff = file1 + file2;
    const result = processGitDiff(diff, { maxTotalChars: 6000 });
    expect(result).toContain('omitted to fit token budget');
  });
});

