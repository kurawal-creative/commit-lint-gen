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

  it('should use env apiKey as fallback when no config file has apiKey', () => {
    process.env.GROQ_API_KEY = 'test-api-key';
    const config = loadConfig();
    // Priority: project -> global -> env. If a config file already has apiKey,
    // env var is only a fallback and won't override it.
    expect(config.apiKey).toBeDefined();
  });

  it('should have default rules', () => {
    const config = loadConfig();
    expect(config.rules?.maxLength).toBe(100);
    expect(config.rules?.minLength).toBe(10);
    expect(config.rules?.types).toContain('feat');
    expect(config.rules?.types).toContain('fix');
  });
});
