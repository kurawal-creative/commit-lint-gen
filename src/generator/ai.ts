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

    const prompt = `Analyze this git diff and generate a conventional commit message following caveman-commit style.

Output format - MUST be valid JSON only:
{
  "type": "feat",
  "scope": "auth",
  "description": "add user authentication",
  "body": "Prevents unauthorized access to protected routes\\n- Adds JWT token validation\\n- Implements session expiry"
}

CRITICAL RULES:
1. Subject line (type + scope + description):
   - ≤50 chars total when possible, hard cap 72
   - Imperative mood: "add", "fix", "remove" — NOT "added/adds/adding"
   - Terse but clear: "add login" not "add user login functionality"
   - No fluff: "now", "currently", "this commit", "functionality"
   
2. Type: [feat, fix, refactor, perf, docs, test, chore, build, ci, style, revert]

3. Scope: Optional, affected area (api, auth, ui, config, generator, etc)

4. Body (ONLY include if needed):
   - Skip entirely when subject is self-explanatory
   - Add ONLY for: non-obvious WHY, breaking changes, migration notes, complex multi-part changes
   - Focus on WHY over WHAT (diff shows what)
   - Use bullets with "-" for lists
   - Wrap at 72 chars per line
   - Use \\n for line breaks in JSON string
   
5. Body examples when needed:
   - Large refactor: explain architectural reason
   - Performance fix: explain bottleneck solved
   - Breaking change: explain migration path
   - Complex feature: explain key parts and rationale
   
6. Body examples to SKIP:
   - Simple addition/deletion (subject says it all)
   - Obvious bug fix (subject + diff clear)
   - Single file change with clear purpose

${previousMessage ? `7. MUST differ significantly from previous: "${previousMessage}"` : ''}

Examples:

Simple (no body needed):
{
  "type": "fix",
  "scope": "auth",
  "description": "resolve token expiry race"
}

Complex (body explains why):
{
  "type": "feat",
  "scope": "generator",
  "description": "add intelligent diff processor",
  "body": "Reduces AI token usage by filtering noise from git diffs:\\n- Skips lockfiles and binary assets (png, pdf, zip)\\n- Strips verbose git metadata (index, ---, +++)\\n- Extracts function context from hunk headers\\n- Enforces 8k char budget per-file with omit counter\\n\\nReplaces naive truncateDiff with configurable processor."
}

Git diff:
${processedDiff}

Output valid JSON only:`;

    const isGroq = config.aiProvider === 'groq';

    const response = await provider.chat.completions.create({
        model: config.model,
        messages: [
            {
                role: 'system',
                content: `You are a JSON API. Respond ONLY with valid JSON. Do not include markdown, explanations, or thinking tags.${previousMessage ? ` IMPORTANT: The previous suggestion was "${previousMessage}". You MUST produce a different type, scope, or description — do not reuse or rephrase it.` : ''}`
            },
            { role: 'user', content: prompt }
        ],
        temperature: 0.7,
        max_completion_tokens: 150,
        response_format: { type: 'json_object' },
        stream: false,
        ...(isGroq && { reasoning_effort: 'none', reasoning_format: 'hidden' }),
    });

    // Type assertion since we explicitly set stream: false
    const chatCompletion = response as Awaited<ReturnType<typeof provider.chat.completions.create>> & { choices: Array<{ message: { content?: string | null } }> };
    const content = chatCompletion.choices[0]?.message?.content;
    if (!content) {
        throw new Error('AI Provider returned empty response')
    }

    try {
        let cleaned = content.trim();

        cleaned = cleaned.replace(/```json?\n?/g, '').replace(/```\n?/g, '');

        cleaned = cleaned.replace(/<think>[\s\S]*?<\/think>/g, '');

        const jsonMatch = cleaned.match(/\{[\s\S]*\}/);
        if (jsonMatch) {
            cleaned = jsonMatch[0];
        }

        const result = JSON.parse(cleaned) as AIResult;

        if (!result.type || !result.description) {
            throw new Error('Invalid AI response: missing required fields')
        }

        return result
    } catch (error) {
        throw new Error(`Failed to parse AI response: ${error instanceof Error ? error.message : String(error)}`)
    }
}