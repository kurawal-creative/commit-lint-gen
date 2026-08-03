import type { SimpleGit } from "simple-git";
import type { Config } from "../config/defaultConfig.js";
import { createAIProvider } from "./provider.js";
import { processGitDiff } from "./diffProcessor.js";

export interface AIResult {
    type: string;
    scope?: string;
    description: string;
    body?: string;
}

const ALLOWED_TYPES = [
    "feat", "fix", "refactor", "perf", "docs",
    "test", "chore", "build", "ci", "style", "revert"
] as const;

export async function generateAICommit(git: SimpleGit, config: Config, previousMessage?: string): Promise<AIResult> {
    const provider = createAIProvider(config);
    const language = config.language || 'en';

    const isRepo = await git.checkIsRepo();
    if (!isRepo) {
        throw new Error('Not a git repository. Run this command from the root of a git repository.');
    }

    const diff = await git.diff(['--cached']);

    if (!diff || diff.trim() === '') {
        throw new Error('No staged changes found. Please stage your changes with `git add` first.');
    }

    const processedDiff = processGitDiff(diff, config.diffProcessor);

    const systemPrompt = `You are an expert commit message generator that carefully analyzes git diffs. Your output MUST be ultra-precise, based on actual code changes, and strictly valid JSON.${language === 'id' ? '\n\nIMPORTANT: Write "description" and "body" fields in Bahasa Indonesia.' : ''}

JSON OUTPUT STRUCTURE:
{
  "type": "one of: ${ALLOWED_TYPES.join(', ')}",
  "scope": "optional lower-case module name or empty string",
  "description": "exact summary of what changed in imperative mood, max 50 chars, no period",
  "body": "detailed explanation of WHY and WHAT changed (wrap at 72 chars, use '-' for bullets), OR empty string if obvious"
}

ANALYSIS RULES:
1. READ EVERY LINE: Examine each +/- line in the diff carefully. Don't guess or assume.
2. IDENTIFY SCOPE: Look at file paths to determine the affected module/component (e.g., api, ui, cli, auth, db).
3. DETERMINE TYPE:
   - feat: New feature/functionality added (+new functions, +new files, +new capabilities)
   - fix: Bug fix (changes to fix errors, edge cases, validation)
   - refactor: Code restructuring without changing behavior (rename, extract, reorganize)
   - perf: Performance improvement (optimization, caching, algorithm change)
   - docs: Documentation only (README, comments, JSDoc)
   - test: Test changes (add/modify tests)
   - chore: Maintenance (deps update, config, tooling)
   - build: Build system (webpack, tsconfig, package.json scripts)
   - ci: CI/CD changes (.github, .gitlab-ci)
   - style: Code style (formatting, whitespace, no logic change)
   - revert: Revert previous commit
4. DESCRIPTION PRECISION: Be specific about WHAT changed. Use actual function/variable/file names if relevant.
   - Good: "add user authentication middleware"
   - Bad: "update code"
   - Good: "fix null pointer in getUserById"
   - Bad: "fix bug"
5. BODY CONTENT: Include WHY if not obvious from description. Mention breaking changes, side effects, or important context.
6. NO FLUFF: Skip phrases like "This commit", "I changed", "now we have". Get straight to the point.
${previousMessage ? `7. CRITICAL: Previous suggestion was "${previousMessage}". You MUST analyze again and output a DIFFERENT and MORE ACCURATE message based on the actual diff.` : ''}`;

    // USER PROMPT: Instruksi eksplisit untuk analisis mendalam
    const userPrompt = `Carefully analyze this git diff and produce an accurate commit message in JSON format.

STEP-BY-STEP ANALYSIS:
1. Read all changed files and identify which modules/components are affected
2. For each file, note what lines were added (+) and removed (-)
3. Determine the primary purpose: is it adding new functionality? fixing a bug? refactoring? updating docs?
4. Choose the most appropriate type and scope based on the actual changes
5. Write a precise description that summarizes the key change

<git_diff>
${processedDiff}
</git_diff>

Based on the above diff, output ONLY valid JSON with type, scope (if applicable), description, and body (if needed).`;

    const isGroq = config.aiProvider === 'groq';

    const response = await provider.chat.completions.create({
        model: config.model,
        messages: [
            { role: 'system', content: systemPrompt },
            { role: 'user', content: userPrompt }
        ],
        // Temperature sedikit lebih tinggi untuk analisis yang lebih baik
        temperature: 0.3,
        max_completion_tokens: 200,
        response_format: { type: 'json_object' },
        stream: false,
        ...(isGroq && { reasoning_effort: 'none', reasoning_format: 'hidden' }),
    });

    const chatCompletion = response as Awaited<ReturnType<typeof provider.chat.completions.create>> & { choices: Array<{ message: { content?: string | null } }> };
    const content = chatCompletion.choices[0]?.message?.content;
    if (!content) {
        throw new Error('AI Provider returned empty response');
    }

    try {
        let cleaned = content.trim();

        // Pembersihan output untuk mengantisipasi kebiasaan model kecil yang kadang menyisipkan markdown/think tags
        cleaned = cleaned.replace(/```json?\n?/gi, '').replace(/```\n?/g, '');
        cleaned = cleaned.replace(/<think>[\s\S]*?<\/think>/gi, '');

        const jsonMatch = cleaned.match(/\{[\s\S]*\}/);
        if (jsonMatch) {
            cleaned = jsonMatch[0];
        }

        const result = JSON.parse(cleaned) as AIResult;

        // Validasi field wajib
        if (!result.type || !result.description) {
            throw new Error('Invalid AI response: missing required fields');
        }

        // Hapus property opsional jika bernilai string kosong
        if (result.scope === '' || result.scope === null) delete result.scope;
        if (result.body === '' || result.body === null) delete result.body;

        // Normalisasi dasar
        result.type = result.type.toLowerCase().trim();
        result.description = result.description.trim();

        return result;
    } catch (error) {
        throw new Error(`Failed to parse AI response: ${error instanceof Error ? error.message : String(error)}`);
    }
}
