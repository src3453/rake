// Project storage for the Rake IDE. A project is a folder under the projects directory:
//
//   <project>/rake-ide.json   IDE manifest ({name, mainScene})
//   <project>/scenes/*.rks    Rake Scene files
//   <project>/src/*.rk        Rake sources
//   <project>/assets/*        costumes, backdrops, and sounds
//
// Deleted projects and files are moved to a `.trash` folder instead of being removed.

import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

export const MANIFEST = 'rake-ide.json';
const HIDDEN_DIRECTORIES = new Set(['.trash', '.rake-build', '.git', 'node_modules']);

export class ProjectError extends Error {
    constructor (status, message) {
        super(message);
        this.status = status;
    }
}

export const validateProjectName = name => {
    const trimmed = typeof name === 'string' ? name.trim() : '';
    if (!trimmed || trimmed.length > 64 || /[<>:"/\\|?*\u0000-\u001f]/.test(trimmed) ||
        trimmed.startsWith('.') || trimmed.endsWith('.')) {
        throw new ProjectError(400, `Invalid project name: ${JSON.stringify(name)}`);
    }
    return trimmed;
};

/** Resolve a project-relative path, rejecting anything outside the project or in hidden folders. */
export const resolveInside = (root, relativePath) => {
    if (typeof relativePath !== 'string' || !relativePath || path.isAbsolute(relativePath) ||
        /^[A-Za-z]:/.test(relativePath)) {
        throw new ProjectError(400, `Invalid path: ${JSON.stringify(relativePath)}`);
    }
    const normalized = path.posix.normalize(relativePath.replace(/\\/g, '/'));
    const first = normalized.split('/')[0];
    if (normalized.startsWith('..') || normalized === '.' || HIDDEN_DIRECTORIES.has(first)) {
        throw new ProjectError(400, `Path is outside the project: ${JSON.stringify(relativePath)}`);
    }
    const resolved = path.resolve(root, normalized);
    const relative = path.relative(root, resolved);
    if (relative.startsWith('..') || path.isAbsolute(relative)) {
        throw new ProjectError(400, `Path is outside the project: ${JSON.stringify(relativePath)}`);
    }
    return {absolute: resolved, relative: normalized};
};

const exists = async file => {
    try {
        await fs.access(file);
        return true;
    } catch {
        return false;
    }
};

const timestamp = () => new Date().toISOString()
    .replace(/[:.]/g, '-');

export class ProjectStore {
    constructor (projectsDirectory, templatesDirectory) {
        this.root = path.resolve(projectsDirectory);
        this.templates = path.resolve(templatesDirectory);
    }

    async init () {
        await fs.mkdir(this.root, {recursive: true});
    }

    projectPath (name) {
        return path.join(this.root, validateProjectName(name));
    }

    async requireProject (name) {
        const directory = this.projectPath(name);
        if (!await exists(directory)) {
            throw new ProjectError(404, `Project not found: ${name}`);
        }
        return directory;
    }

    async readManifest (directory) {
        try {
            return JSON.parse(await fs.readFile(path.join(directory, MANIFEST), 'utf8'));
        } catch {
            return {};
        }
    }

    async list () {
        const entries = await fs.readdir(this.root, {withFileTypes: true});
        const projects = [];
        for (const entry of entries) {
            if (!entry.isDirectory() || entry.name.startsWith('.')) continue;
            const directory = path.join(this.root, entry.name);
            const manifest = await this.readManifest(directory);
            const stat = await fs.stat(directory);
            projects.push({
                name: entry.name,
                mainScene: manifest.mainScene || null,
                modified: stat.mtimeMs
            });
        }
        projects.sort((a, b) => b.modified - a.modified);
        return projects;
    }

    async templateNames () {
        const entries = await fs.readdir(this.templates, {withFileTypes: true});
        return entries.filter(entry => entry.isDirectory()).map(entry => entry.name);
    }

    async create (name, template = 'default') {
        const directory = this.projectPath(name);
        if (await exists(directory)) {
            throw new ProjectError(409, `A project named ${name} already exists`);
        }
        const templates = await this.templateNames();
        if (!templates.includes(template)) {
            throw new ProjectError(400, `Unknown template: ${template}`);
        }
        await fs.cp(path.join(this.templates, template), directory, {recursive: true});
        const manifest = await this.readManifest(directory);
        await this.writeManifest(name, {...manifest, name: path.basename(directory)});
        return this.describe(name);
    }

    /**
     * Create a project by decompiling an .sb3 with `decompile(sb3Path, directory)`.
     * The project folder is removed again when decompilation fails.
     */
    async importSb3 (name, bytes, decompile) {
        const directory = this.projectPath(name);
        if (await exists(directory)) {
            throw new ProjectError(409, `A project named ${name} already exists`);
        }
        if (!bytes.length || bytes[0] !== 0x50 || bytes[1] !== 0x4b) {
            throw new ProjectError(400, 'The uploaded file is not an .sb3 archive');
        }
        const staging = await fs.mkdtemp(path.join(os.tmpdir(), 'rake-import-'));
        try {
            const sb3Path = path.join(staging, 'project.sb3');
            await fs.writeFile(sb3Path, bytes);
            const result = await decompile(sb3Path, directory);
            if (!result.ok) {
                await fs.rm(directory, {recursive: true, force: true});
                throw new ProjectError(422, `Cannot decompile the .sb3: ${result.error}`);
            }
            const manifest = await this.readManifest(directory);
            await this.writeManifest(name, {...manifest, name: path.basename(directory)});
            return {project: await this.describe(name), warnings: result.warnings};
        } finally {
            await fs.rm(staging, {recursive: true, force: true});
        }
    }

    async duplicate (name, newName) {
        const source = await this.requireProject(name);
        const target = this.projectPath(newName);
        if (await exists(target)) {
            throw new ProjectError(409, `A project named ${newName} already exists`);
        }
        await fs.cp(source, target, {
            recursive: true,
            filter: file => !path.relative(source, file).split(path.sep)
                .some(part => part === '.trash' || part === '.rake-build')
        });
        const manifest = await this.readManifest(target);
        await this.writeManifest(newName, {...manifest, name: path.basename(target)});
        return this.describe(newName);
    }

    async rename (name, newName) {
        const source = await this.requireProject(name);
        const target = this.projectPath(newName);
        if (await exists(target)) {
            throw new ProjectError(409, `A project named ${newName} already exists`);
        }
        await fs.rename(source, target);
        const manifest = await this.readManifest(target);
        await this.writeManifest(newName, {...manifest, name: path.basename(target)});
        return this.describe(newName);
    }

    /** Move a project to `<projects>/.trash` so it can be recovered by hand. */
    async trash (name) {
        const source = await this.requireProject(name);
        const trash = path.join(this.root, '.trash');
        await fs.mkdir(trash, {recursive: true});
        const target = path.join(trash, `${timestamp()}-${path.basename(source)}`);
        await fs.rename(source, target);
        return {trashedTo: target};
    }

    async writeManifest (name, manifest) {
        const directory = await this.requireProject(name);
        const clean = {
            name: typeof manifest.name === 'string' ? manifest.name : path.basename(directory),
            mainScene: typeof manifest.mainScene === 'string' ? manifest.mainScene : null
        };
        if (clean.mainScene) resolveInside(directory, clean.mainScene);
        await fs.writeFile(path.join(directory, MANIFEST), `${JSON.stringify(clean, null, 2)}\n`);
        return clean;
    }

    async files (directory) {
        const result = [];
        const walk = async relative => {
            const entries = await fs.readdir(path.join(directory, relative), {withFileTypes: true});
            for (const entry of entries) {
                if (HIDDEN_DIRECTORIES.has(entry.name)) continue;
                const child = relative ? `${relative}/${entry.name}` : entry.name;
                if (entry.isDirectory()) {
                    result.push({path: child, directory: true});
                    await walk(child);
                } else if (entry.isFile()) {
                    const stat = await fs.stat(path.join(directory, child));
                    result.push({path: child, directory: false, size: stat.size, modified: stat.mtimeMs});
                }
            }
        };
        await walk('');
        result.sort((a, b) => a.path.localeCompare(b.path));
        return result;
    }

    async describe (name) {
        const directory = await this.requireProject(name);
        return {
            name: path.basename(directory),
            root: directory,
            manifest: await this.readManifest(directory),
            files: await this.files(directory)
        };
    }

    async readFile (name, relativePath) {
        const directory = await this.requireProject(name);
        const {absolute} = resolveInside(directory, relativePath);
        try {
            return await fs.readFile(absolute);
        } catch {
            throw new ProjectError(404, `File not found: ${relativePath}`);
        }
    }

    async writeFile (name, relativePath, contents) {
        const directory = await this.requireProject(name);
        const {absolute, relative} = resolveInside(directory, relativePath);
        await fs.mkdir(path.dirname(absolute), {recursive: true});
        await fs.writeFile(absolute, contents);
        return {path: relative};
    }

    async makeDirectory (name, relativePath) {
        const directory = await this.requireProject(name);
        const {absolute, relative} = resolveInside(directory, relativePath);
        await fs.mkdir(absolute, {recursive: true});
        return {path: relative};
    }

    async moveFile (name, from, to) {
        const directory = await this.requireProject(name);
        const source = resolveInside(directory, from);
        const target = resolveInside(directory, to);
        if (!await exists(source.absolute)) throw new ProjectError(404, `File not found: ${from}`);
        if (await exists(target.absolute)) throw new ProjectError(409, `${to} already exists`);
        await fs.mkdir(path.dirname(target.absolute), {recursive: true});
        await fs.rename(source.absolute, target.absolute);
        return {path: target.relative};
    }

    /** Move a file or folder into `<project>/.trash`. */
    async trashFile (name, relativePath) {
        const directory = await this.requireProject(name);
        const {absolute} = resolveInside(directory, relativePath);
        if (!await exists(absolute)) throw new ProjectError(404, `File not found: ${relativePath}`);
        const trash = path.join(directory, '.trash');
        await fs.mkdir(trash, {recursive: true});
        const target = path.join(trash, `${timestamp()}-${path.basename(absolute)}`);
        await fs.rename(absolute, target);
        return {trashedTo: target};
    }
}
