import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { loadConfig } from '../../src/config/loader.js';

describe('loadConfig', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.resetModules();
    process.env = { ...originalEnv };
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('should load default config when no config file exists', () => {
    const config = loadConfig();
    expect(config.aiProvider).toBe('groq');
    expect(config.baseURL).toBe('https://api.groq.com/openai/v1');
    expect(config.model).toBe('qwen/qwen3.6-27b');
  });

  it('should merge environment variables', () => {
    process.env.GROQ_API_KEY = 'test-api-key';
    const config = loadConfig();
    expect(config.apiKey).toBe('test-api-key');
  });

  it('should have default rules', () => {
    const config = loadConfig();
    expect(config.rules?.maxLength).toBe(100);
    expect(config.rules?.minLength).toBe(5);
    expect(config.rules?.types).toContain('feat');
    expect(config.rules?.types).toContain('fix');
  });
});
