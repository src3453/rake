// Stage -> Rake Scene export: writes the VM's targets (initial state, costumes, sounds)
// to a `.rks` file and its asset files, keeping each target's script assignments.

const posix = {
    dirname: p => (p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : ''),
    join: (...parts) => parts.filter(Boolean).join('/')
        .replace(/\/+/g, '/'),
    normalize (p) {
        const out = [];
        for (const part of p.split('/')) {
            if (!part || part === '.') continue;
            if (part === '..' && out.length && out[out.length - 1] !== '..') out.pop();
            else out.push(part);
        }
        return out.join('/');
    },
    relative (from, to) {
        const a = from ? from.split('/') : [];
        const b = to.split('/');
        let common = 0;
        while (common < a.length && common < b.length && a[common] === b[common]) common++;
        return [...Array(a.length - common).fill('..'), ...b.slice(common)].join('/');
    }
};

export const resolveScenePath = (scenePath, reference) =>
    posix.normalize(posix.join(posix.dirname(scenePath), reference));

export const sceneRelativePath = (scenePath, projectPath) =>
    posix.relative(posix.dirname(scenePath), projectPath);

/** Make a name safe as a file name and a Rake identifier (sprites must be identifiers). */
export const safeFileName = name => (name || 'asset').replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_')
    .replace(/^\.+/, '_')
    .trim() || 'asset';

export const safeIdentifier = name => {
    const cleaned = (name || 'Sprite').replace(/[^A-Za-z0-9_]/g, '_');
    return /^[A-Za-z_]/.test(cleaned) ? cleaned : `_${cleaned}`;
};

const number = value => {
    const rounded = Math.round(Number(value) * 1000) / 1000;
    return Number.isFinite(rounded) ? String(rounded) : '0';
};

const quote = text => `"${String(text).replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')}"`;

const assetEntry = asset => {
    if (!asset.center) return quote(asset.path);
    const parts = [quote(asset.path), number(asset.center[0]), number(asset.center[1])];
    if (asset.resolution && asset.resolution !== 1) parts.push(String(asset.resolution));
    return `(${parts.join(', ')})`;
};

const list = (items, indent) => {
    if (!items.length) return '[]';
    if (items.length === 1) return `[${items[0]}]`;
    return `[\n${items.map(item => `${indent}    ${item},`).join('\n')}\n${indent}]`;
};

/** Format a scene model as `.rks` source. */
export const formatScene = scene => {
    const lines = [`// Exported from the Stage by the Rake IDE.`, `scene ${scene.name} {`];
    const stage = scene.stage;
    lines.push('    stage {');
    if (stage.backdrops.length) {
        lines.push(`        backdrops: ${list(stage.backdrops.map(assetEntry), '        ')}`);
        if (stage.currentBackdrop) lines.push(`        current_backdrop: ${stage.currentBackdrop}`);
    }
    if (stage.sounds.length) lines.push(`        sounds: ${list(stage.sounds.map(sound => quote(sound.path)), '        ')}`);
    if (stage.scripts.length) lines.push(`        scripts: ${list(stage.scripts.map(quote), '        ')}`);
    if (stage.volume !== 100) lines.push(`        volume: ${number(stage.volume)}`);
    if (stage.tempo !== 60) lines.push(`        tempo: ${number(stage.tempo)}`);
    lines.push('    }');
    for (const sprite of scene.sprites) {
        lines.push('', `    sprite ${sprite.name} {`);
        if (sprite.costumes.length) {
            lines.push(`        costumes: ${list(sprite.costumes.map(assetEntry), '        ')}`);
            if (sprite.currentCostume) lines.push(`        current_costume: ${sprite.currentCostume}`);
        }
        if (sprite.sounds.length) lines.push(`        sounds: ${list(sprite.sounds.map(sound => quote(sound.path)), '        ')}`);
        lines.push(`        position: (${number(sprite.x)}, ${number(sprite.y)})`);
        lines.push(`        size: ${number(sprite.size)}`);
        lines.push(`        direction: ${number(sprite.direction)}`);
        if (!sprite.visible) lines.push('        visible: false');
        lines.push(`        layer: ${sprite.layer}`);
        if (sprite.draggable) lines.push('        draggable: true');
        if (sprite.rotationStyle !== 'all around') lines.push(`        rotation_style: ${quote(sprite.rotationStyle)}`);
        if (sprite.volume !== 100) lines.push(`        volume: ${number(sprite.volume)}`);
        if (sprite.scripts.length) lines.push(`        scripts: ${list(sprite.scripts.map(quote), '        ')}`);
        lines.push('    }');
    }
    lines.push('}', '');
    return lines.join('\n');
};

const bytesEqual = (a, b) => {
    if (!a || !b || a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
    return true;
};

/**
 * Export the VM's original targets into `scenePath`.
 *
 * `previous` is the parsed current scene (or null). Script lists are taken from it by
 * target name, and its asset paths are reused for assets with the same name, so a
 * round-trip keeps the project's file layout. New assets go to `assets/<Scene>/<Target>/`.
 */
export const exportStageToScene = async ({vm, api, project, scenePath, sceneName, previous}) => {
    const previousSprites = new Map(((previous && previous.sprites) || []).map(sprite => [sprite.name, sprite]));
    const previousStage = previous ? previous.stage : null;
    const written = [];
    const skipped = [];

    const existingPath = (previousAssets, name) => {
        const match = (previousAssets || []).find(asset => asset.name === name);
        return match ? resolveScenePath(scenePath, match.path) : null;
    };

    const writeAsset = async (projectPath, data) => {
        let current = null;
        try {
            current = await api.readBytes(project, projectPath);
        } catch (e) {
            current = null;
        }
        if (!bytesEqual(current, data)) {
            await api.writeFile(project, projectPath, new Blob([data]));
            written.push(projectPath);
        }
        return sceneRelativePath(scenePath, projectPath);
    };

    const exportCostumes = async (target, folder, previousAssets) => {
        const result = [];
        for (const costume of target.getCostumes()) {
            const asset = costume.asset;
            const extension = (asset.dataFormat || 'png').toLowerCase();
            const name = safeFileName(costume.name);
            const reuse = existingPath(previousAssets, costume.name);
            const projectPath = reuse && reuse.toLowerCase().endsWith(`.${extension}`) ?
                reuse : posix.join(folder, `${name}.${extension}`);
            const relative = await writeAsset(projectPath, asset.data);
            const resolution = costume.bitmapResolution || 1;
            result.push({
                path: relative,
                center: [costume.rotationCenterX, costume.rotationCenterY],
                resolution: extension === 'svg' ? 1 : resolution
            });
        }
        return result;
    };

    const exportSounds = async (target, folder, previousAssets) => {
        const result = [];
        for (const sound of target.getSounds()) {
            const asset = sound.asset;
            const extension = (asset.dataFormat || 'wav').toLowerCase();
            if (extension !== 'wav' && extension !== 'mp3') {
                skipped.push(`${target.getName()}: ${sound.name}`);
                continue;
            }
            const reuse = existingPath(previousAssets, sound.name);
            const projectPath = reuse || posix.join(folder, `${safeFileName(sound.name)}.${extension}`);
            result.push({path: await writeAsset(projectPath, asset.data)});
        }
        return result;
    };

    const targets = vm.runtime.targets.filter(target => target.isOriginal);
    const stageTarget = targets.find(target => target.isStage);
    const assetRoot = posix.join('assets', safeFileName(sceneName));

    const stage = {
        backdrops: await exportCostumes(stageTarget, posix.join(assetRoot, 'Stage'),
            previousStage && previousStage.backdrops),
        currentBackdrop: stageTarget.currentCostume,
        sounds: await exportSounds(stageTarget, posix.join(assetRoot, 'Stage'), previousStage && previousStage.sounds),
        scripts: previousStage ? previousStage.scripts : [],
        volume: stageTarget.volume,
        tempo: stageTarget.tempo || 60
    };

    const sprites = [];
    const usedNames = new Set();
    const ordered = targets.filter(target => !target.isStage)
        .sort((a, b) => a.getLayerOrder() - b.getLayerOrder());
    const layers = new Map(ordered.map((target, index) => [target, index + 1]));
    for (const target of targets.filter(candidate => !candidate.isStage)) {
        let name = safeIdentifier(target.getName());
        while (usedNames.has(name)) name = `${name}_`;
        usedNames.add(name);
        const before = previousSprites.get(target.getName()) || previousSprites.get(name);
        const folder = posix.join(assetRoot, safeFileName(name));
        sprites.push({
            name,
            costumes: await exportCostumes(target, folder, before && before.costumes),
            currentCostume: target.currentCostume,
            sounds: await exportSounds(target, folder, before && before.sounds),
            x: target.x,
            y: target.y,
            size: target.size,
            direction: target.direction,
            visible: target.visible,
            layer: layers.get(target),
            draggable: target.draggable,
            rotationStyle: target.rotationStyle,
            volume: target.volume,
            scripts: before ? before.scripts : []
        });
    }

    const text = formatScene({name: safeIdentifier(sceneName), stage, sprites});
    await api.writeFile(project, scenePath, text);
    return {text, written, skipped, sprites: sprites.map(sprite => sprite.name)};
};

/** A new, empty scene file. */
export const newSceneSource = name => `scene ${safeIdentifier(name)} {\n    stage {\n    }\n}\n`;
