export interface Config {
    aiProvider: string;
    baseURL: string;
    model: string;
    apiKey?: string;
    language?: 'en' | 'id';

    rules?: {
        maxLength?: number;
        minLength?: number;
        types?: string[];
        scopes?: string[];
        requireScope?: boolean;
    }

    diffProcessor?: {
        maxTotalChars?: number;
        ignoredExts?: string[];
        ignoreLocks?: boolean;
    }
}

export const defaultConfig: Config = {
    aiProvider: 'groq',
    baseURL: 'https://api.groq.com/openai/v1',
    model: 'qwen/qwen3.6-27b',
    language: 'en',
    rules: {
        maxLength: 100,
        minLength: 5,
        types: ['feat', 'fix', 'docs', 'style', 'refactor', 'test', 'chore', 'ci', 'build', 'perf', 'revert'],
        requireScope: false
    },
    diffProcessor: {
        maxTotalChars: 8000,
        ignoredExts: ['.png', '.jpg', '.jpeg', '.gif', '.svg', '.ico', '.pdf', '.zip', '.tar', '.gz', '.wasm', '.mp4'],
        ignoreLocks: true
    }
}