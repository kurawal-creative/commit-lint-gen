import { writeFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { SimpleGit } from 'simple-git';
import { generateHeuristicCommit } from './heuristic.js';
import { generateAICommit } from './ai.js';
import { promptCommitAction, editDraft, manualEntry } from './interactive.js';
import type { Config } from '../config/defaultConfig.js';

function formatMessage(type: string, scope: string | undefined, description: string): string {
    return scope ? `${type}(${scope}): ${description}` : `${type}: ${description}`;
}

async function generateCommit(git: SimpleGit, config: Config, useAI: boolean, forceHeuristic?: boolean, previousMessage?: string) {
    if (!forceHeuristic && useAI && config.apiKey) {
        try {
            const result = await generateAICommit(git, config, previousMessage);
            return { ...result, confidence: 'high' as const };
        } catch (error) {
            console.log(`AI generation failed: ${error instanceof Error ? error.message : String(error)}`)
        }
    }

    return await generateHeuristicCommit(git);
}

/**
 * Main loop: generate draft -> preview -> wait user action -> repeat
 * until user press accept (write into commitMsgFile) or cancel (exit non-zero).
 *
 * commitMsgFile is filled if called by git hook prepare-commit-msg
 * (git gives file path through first argument). If called manually
 * through `clg generate` without hook, commitMsgFile undefined -> end result
 * only printed to stdout, not written to any file.
 */
export async function runInteractiveGenerate(git: SimpleGit, config: Config, commitMsgFile?: string, autoYes?: boolean, forceHeuristic?: boolean): Promise<void> {
    const useAI = !!config.apiKey;
    const frames = ['⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏'];
    const spin = (msg: string) => {
        let fi = 0;
        const id = setInterval(() => process.stdout.write(`\r${frames[fi++ % frames.length]} ${msg}`), 80);
        return () => { clearInterval(id); process.stdout.write('\r\x1b[K'); };
    };

    const stopSpin = spin('Generating...');
    const initial = await generateCommit(git, config, useAI, forceHeuristic);
    stopSpin();
    let draft = formatMessage(initial.type, initial.scope, initial.description);
    let confidence: string | undefined = initial.confidence;

    if (autoYes) {
        await git.commit(draft);
        console.log(`Committed: ${draft}`);
        console.log(`\nDon't forget to push your commits!`);
        return;
    }

    while (true) {
        const currentLang = config.language || 'en';
        const action = await promptCommitAction(draft, confidence, currentLang);

        if (action === 'accept') {
            if (commitMsgFile) {
                writeFileSync(commitMsgFile, draft + '\n');
            } else {
                await git.commit(draft);
                console.log(`\nCommitted: ${draft}`);
                console.log(`\nDon't forget to push your commits!`);
            }
            return;
        }

        if (action === 'edit') {
            const edited = await editDraft(draft);
            if (edited !== null) {
                draft = edited;
                confidence = undefined;
            }
            continue;
        }

        if (action === 'regenerate') {
            const stopSpin = spin('Regenerating...');
            const result = await generateCommit(git, config, useAI, forceHeuristic, draft);
            stopSpin();
            draft = formatMessage(result.type, result.scope, result.description);
            confidence = result.confidence;
            continue;
        }

        if (action === 'language') {
            config.language = currentLang === 'en' ? 'id' : 'en';
            await updateConfigFile(config);
            const stopSpin = spin('Regenerating with new language...');
            const result = await generateCommit(git, config, useAI, forceHeuristic, draft);
            stopSpin();
            draft = formatMessage(result.type, result.scope, result.description);
            confidence = result.confidence;
            continue;
        }

        if (action === 'manual') {
            const manual = await manualEntry();
            if (manual !== null) {
                draft = manual;
                confidence = undefined;
            }
            continue;
        }

        if (action === 'cancel') {
            console.log('Cancelled, did not continue commit.');
            process.exit(1);
        }
    }
}

async function updateConfigFile(config: Config): Promise<void> {
    const { cosmiconfig } = await import('cosmiconfig');
    const explorer = cosmiconfig('commitlintgen');
    const fs = await import('node:fs/promises');

    const globalConfig = join(homedir(), '.commitlintgenrc.json');
    const searchResult = await explorer.search();

    let filepath: string;
    let existingConfig: Record<string, unknown>;

    if (searchResult && searchResult.filepath) {
        filepath = searchResult.filepath;
        existingConfig = searchResult.config || {};
    } else if (existsSync(globalConfig)) {
        const loaded = await explorer.load(globalConfig);
        filepath = globalConfig;
        existingConfig = loaded?.config || {};
    } else {
        // no config file found anywhere, nothing to update
        return;
    }

    await fs.writeFile(
        filepath,
        JSON.stringify({
            ...existingConfig,
            language: config.language
        }, null, 2) + '\n'
    );
}