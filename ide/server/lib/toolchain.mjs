// Locates and runs the Rake compiler and language server binaries.

import {spawn} from 'node:child_process';
import fs from 'node:fs';
import fsp from 'node:fs/promises';
import path from 'node:path';

const executable = name => (process.platform === 'win32' ? `${name}.exe` : name);

/** Prefer an explicit environment variable, then the newest Cargo build in the repository. */
export const findBinary = (repositoryRoot, name, override) => {
    if (override) return override;
    const candidates = ['release', 'debug']
        .map(profile => path.join(repositoryRoot, 'target', profile, executable(name)))
        .filter(candidate => fs.existsSync(candidate))
        .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs);
    if (!candidates.length) {
        throw new Error(`Cannot find ${executable(name)}. Run \`cargo build --bins\` in ${repositoryRoot} ` +
            `or set the ${name === 'rake' ? 'RAKE_BIN' : 'RAKE_LSP_BIN'} environment variable.`);
    }
    return candidates[0];
};

const run = (command, args, cwd) => new Promise((resolve, reject) => {
    const child = spawn(command, args, {cwd, windowsHide: true});
    const stdout = [];
    const stderr = [];
    child.stdout.on('data', chunk => stdout.push(chunk));
    child.stderr.on('data', chunk => stderr.push(chunk));
    child.on('error', reject);
    child.on('close', code => resolve({
        code,
        stdout: Buffer.concat(stdout).toString('utf8'),
        stderr: Buffer.concat(stderr).toString('utf8')
    }));
});

/** Turn `--json-errors` output into a diagnostic with a project-relative file path. */
export const parseCompilerError = (stderr, projectRoot) => {
    const line = stderr.split(/\r?\n/).find(text => text.trim().startsWith('{'));
    if (line) {
        try {
            const error = JSON.parse(line);
            const absolute = path.resolve(projectRoot, error.file || '');
            const relative = path.relative(projectRoot, absolute).split(path.sep)
                .join('/');
            return {
                file: relative.startsWith('..') ? error.file : relative,
                line: error.line || 1,
                column: error.column || 1,
                message: error.message || 'compilation failed'
            };
        } catch {
            // Fall through to the plain-text error.
        }
    }
    return {file: null, line: 1, column: 1, message: stderr.trim() || 'compilation failed'};
};

export class Toolchain {
    constructor (repositoryRoot, {rakeBin, lspBin} = {}) {
        this.repositoryRoot = repositoryRoot;
        this.rakeOverride = rakeBin;
        this.lspOverride = lspBin;
        this.queues = new Map();
    }

    get rake () {
        return findBinary(this.repositoryRoot, 'rake', this.rakeOverride);
    }

    get lsp () {
        return findBinary(this.repositoryRoot, 'rake-lsp', this.lspOverride);
    }

    /** Builds of one project run one at a time so they never share the output file. */
    serialize (key, task) {
        const previous = this.queues.get(key) || Promise.resolve();
        const next = previous.catch(() => {}).then(task);
        this.queues.set(key, next);
        return next;
    }

    /** Compile a scene (or single .rk file) to an .sb3; resolves to {ok, sb3} or {ok: false, error}. */
    build (projectRoot, sourcePath) {
        return this.serialize(projectRoot, async () => {
            const outputDirectory = path.join(projectRoot, '.rake-build');
            await fsp.mkdir(outputDirectory, {recursive: true});
            const output = path.join(outputDirectory, `${path.basename(sourcePath).replace(/\.[^.]+$/, '')}.sb3`);
            const started = Date.now();
            const result = await run(this.rake, [sourcePath, '-o', output, '--json-errors'], projectRoot);
            if (result.code !== 0) {
                return {ok: false, error: parseCompilerError(result.stderr, projectRoot), elapsed: Date.now() - started};
            }
            const blocks = Number((/\((\d+) Scratch blocks\)/.exec(result.stdout) || [])[1] || 0);
            return {ok: true, sb3: await fsp.readFile(output), blocks, elapsed: Date.now() - started};
        });
    }

    /**
     * Decompile an .sb3 into a new Rake project directory (experimental).
     * Resolves to {ok: true, warnings} or {ok: false, error}.
     */
    async decompile (sb3Path, outputDirectory) {
        const result = await run(this.rake, ['decompile', sb3Path, '-o', outputDirectory], path.dirname(sb3Path));
        const lines = result.stderr.split(/\r?\n/);
        const warnings = lines.filter(line => line.startsWith('warning: ')).map(line => line.slice('warning: '.length));
        if (result.code !== 0) {
            const errors = lines.filter(line => line.startsWith('error: ')).map(line => line.slice('error: '.length));
            return {ok: false, error: errors.join('\n') || result.stderr.trim() || 'decompilation failed'};
        }
        return {ok: true, warnings};
    }

    /** Parse a scene into JSON (sprites, scripts, assets) without compiling it. */
    async describeScene (projectRoot, scenePath) {
        const result = await run(this.rake, [scenePath, '--print-scene', '--json-errors'], projectRoot);
        if (result.code !== 0) {
            return {ok: false, error: parseCompilerError(result.stderr, projectRoot)};
        }
        return {ok: true, scene: JSON.parse(result.stdout)};
    }
}
