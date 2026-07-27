import type { SimpleGit } from "simple-git";
import type { Config } from "../config/defaultConfig.js";
import { createAIProvider } from "./provider.js";
import { processGitDiff } from "./diffProcessor.js";

export interface AIResult {
    type: string;
    scope?: string;
    description: string;
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

    const prompt = `Analyze this git diff and generate a conventional commit message.

Output format - MUST be valid JSON only, no markdown, no explanations, no thinking tags:
{
  "type": "feat",
  "scope": "auth",
  "description": "add user login functionality"
}

Rules:
- type: One of [feat, fix, docs, style, refactor, test, chore, ci]
- scope: Optional, the area affected (e.g., "api", "auth", "ui")
- description: Short imperative phrase (e.g., "add feature" not "added" or "adds")
${previousMessage ? `- Do NOT reuse or slightly rephrase this previous message: "${previousMessage}". Use a different description.` : ''}
Git diff:
${processedDiff}

Output the JSON object only:`;

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
        // Clean up the response - remove markdown code blocks, thinking tags, and extra text
        let cleaned = content.trim();

        // Remove markdown code blocks
        cleaned = cleaned.replace(/```json?\n?/g, '').replace(/```\n?/g, '');

        cleaned = cleaned.replace(/<think>[\s\S]*?<\/think>/g, '');

        // Try to extract JSON object if there's surrounding text
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