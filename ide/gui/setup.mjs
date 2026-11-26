// Prepares and builds the Rake IDE editor from TurboWarp's scratch-gui.
//
//   bun gui/setup.mjs           clone (pinned commit), install, apply Rake changes, build
//   bun gui/setup.mjs --build   re-apply Rake changes and rebuild only
//
// Dependencies are installed with Bun and webpack runs on the Bun runtime.
//
// The TurboWarp checkout lives in ide/gui/scratch-gui (not committed). Rake's changes are
// ide/gui/overlay (new files copied in) and ide/gui/patches/*.patch (edits to GUI files).

import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const TURBOWARP_REPOSITORY = 'https://github.com/TurboWarp/scratch-gui.git';
const TURBOWARP_COMMIT = '28132778f766e97d1cc7553bc5e4f269c6737bb8';

const here = path.dirname(fileURLToPath(import.meta.url));
const checkout = path.join(here, 'scratch-gui');
const buildOnly = process.argv.includes('--build');
const production = process.argv.includes('--production');

const run = (command, args, options = {}) => {
    console.log(`> ${command} ${args.join(' ')}`);
    const result = spawnSync(command, args, {stdio: 'inherit', shell: process.platform === 'win32', ...options});
    if (result.status !== 0) {
        throw new Error(`${command} ${args.join(' ')} failed with exit code ${result.status}`);
    }
};

const quiet = (command, args, options = {}) =>
    spawnSync(command, args, {encoding: 'utf8', shell: process.platform === 'win32', ...options});

if (!buildOnly) {
    if (!fs.existsSync(checkout)) {
        run('git', ['clone', '--filter=blob:none', '--no-checkout', TURBOWARP_REPOSITORY, checkout]);
    }
    run('git', ['fetch', '--depth', '1', 'origin', TURBOWARP_COMMIT], {cwd: checkout});
    // Discard earlier Rake edits so the patch applies to clean files.
    run('git', ['checkout', '--force', TURBOWARP_COMMIT], {cwd: checkout});
    // Bun migrates TurboWarp's package-lock.json, keeping its exact versions. The lock's
    // integrity hashes for TurboWarp's GitHub forks (scratch-render, scratch-paint, ...) are
    // hashes of npm-packed tarballs, which never match the GitHub archives Bun downloads,
    // so drop them; those packages stay pinned by commit in `resolved`.
    const lockPath = path.join(checkout, 'package-lock.json');
    const lock = JSON.parse(fs.readFileSync(lockPath, 'utf8'));
    for (const entry of Object.values(lock.packages || {})) {
        if (String(entry.resolved || '').startsWith('git+')) delete entry.integrity;
    }
    fs.writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}
`);
    fs.rmSync(path.join(checkout, 'bun.lock'), {force: true});
    run('bun', ['install'], {cwd: checkout});
}

// Copy Rake's own source files into the checkout.
fs.cpSync(path.join(here, 'overlay'), checkout, {recursive: true});

// Apply edits to TurboWarp files unless they are already applied.
for (const patch of fs.readdirSync(path.join(here, 'patches')).filter(name => name.endsWith('.patch'))) {
    const file = path.join(here, 'patches', patch);
    const flags = ['--ignore-whitespace'];
    if (quiet('git', ['apply', '--reverse', '--check', ...flags, file], {cwd: checkout}).status === 0) {
        console.log(`${patch}: already applied`);
        continue;
    }
    run('git', ['apply', ...flags, file], {cwd: checkout});
}

run('bun', ['x', '--bun', 'webpack', '--bail'], {
    cwd: checkout,
    env: {...process.env, CI: '1', NODE_ENV: production ? 'production' : 'development'}
});
console.log(`\nBuilt the Rake IDE editor into ${path.join(checkout, 'build')}`);
