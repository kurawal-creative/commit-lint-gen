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

    const isRepo = await git.checkIsRepo();
    if (!isRepo) {
        throw new Error('Not a git repository. Run this command from the root of a git repository.');
    }

    const diff = await git.diff(['--cached']);

    if (!diff || diff.trim() === '') {
        throw new Error('No staged changes found. Please stage your changes with `git add` first.');
    }

    const processedDiff = processGitDiff(diff, config.diffProcessor);

    // SYSTEM PROMPT: Menerjemahkan aturan caveman-commit secara terstruktur & ketat untuk model kecil
    const systemPrompt = `You are a caveman-style commit message generator. Your output MUST be ultra-compressed, exact, and strictly valid JSON.

JSON OUTPUT STRUCTURE:
{
  "type": "one of: ${ALLOWED_TYPES.join(', ')}",
  "scope": "optional lower-case module name or empty string",
  "description": "terse, exact summary in imperative mood, max 50 chars, no period",
  "body": "reason 'why' change was made (wrap at 72 chars, use '-' for bullets), OR empty string if self-explanatory"
}

CAVEMAN COMMIT RULES:
1. FOCUS ON WHY OVER WHAT: The diff shows what changed; write WHY it changed.
2. SUBJECT LINE: Imperative mood ("add", "fix", "remove" — NOT "added", "adds"). No trailing period. ≤50 chars.
3. BODY RULE: Skip body ENTIRELY if subject is self-explanatory. ONLY add body for non-obvious "why", breaking changes, security fixes, or migrations.
4. FORBIDDEN WORDS: NEVER use "This commit", "I", "we", "now", "currently", or AI attribution ("Generated with...").
5. NO EMOJIS. No file names in description if scope covers it.
${previousMessage ? `6. CRITICAL: Previous suggestion was "${previousMessage}". You MUST output a completely different type, scope, or description.` : ''}`;

    // USER PROMPT: Menyajikan git diff dengan separator yang bersih
    const userPrompt = `Analyze this git diff and produce a caveman-style commit JSON:

<git_diff>
${processedDiff}
</git_diff>`;

    const isGroq = config.aiProvider === 'groq';

    const response = await provider.chat.completions.create({
        model: config.model,
        messages: [
            { role: 'system', content: systemPrompt },
            { role: 'user', content: userPrompt }
        ],
        // Temperature rendah (0.15) sangat krusial agar model kecil konsisten mengikuti gaya ultra-terse
        temperature: 0.15,
        max_completion_tokens: 150,
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
