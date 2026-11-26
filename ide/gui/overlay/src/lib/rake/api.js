// REST client for the local Rake IDE server (ide/server).

const encodePath = relativePath => relativePath.split('/').map(encodeURIComponent)
    .join('/');

export class RakeApiError extends Error {
    constructor (status, body) {
        super((body && body.error && body.error.message) || `Request failed (${status})`);
        this.status = status;
        this.body = body;
    }
}

const request = async (method, url, {json, body, raw} = {}) => {
    const response = await fetch(url, {
        method,
        headers: json === undefined ? {} : {'Content-Type': 'application/json'},
        body: json === undefined ? body : JSON.stringify(json),
        cache: 'no-store'
    });
    if (raw) return response;
    const text = await response.text();
    const value = text ? JSON.parse(text) : null;
    if (!response.ok) throw new RakeApiError(response.status, value);
    return value;
};

const project = name => `/api/projects/${encodeURIComponent(name)}`;

export default {
    info: () => request('GET', '/api/info'),
    listProjects: () => request('GET', '/api/projects'),
    createProject: (name, template = 'default') => request('POST', '/api/projects', {json: {name, template}}),
    /** Experimental: decompile an .sb3 file into a new project. Resolves to {project, warnings}. */
    importSb3: (name, file) => request('POST', `/api/import?name=${encodeURIComponent(name)}`, {body: file}),
    getProject: name => request('GET', project(name)),
    renameProject: (name, newName) => request('POST', `${project(name)}/rename`, {json: {name: newName}}),
    duplicateProject: (name, newName) => request('POST', `${project(name)}/duplicate`, {json: {name: newName}}),
    trashProject: name => request('DELETE', project(name)),
    writeManifest: (name, manifest) => request('PUT', `${project(name)}/manifest`, {json: manifest}),

    async readText (name, relativePath) {
        const response = await request('GET', `${project(name)}/files/${encodePath(relativePath)}`, {raw: true});
        if (!response.ok) throw new RakeApiError(response.status, null);
        return response.text();
    },
    async readBytes (name, relativePath) {
        const response = await request('GET', `${project(name)}/files/${encodePath(relativePath)}`, {raw: true});
        if (!response.ok) throw new RakeApiError(response.status, null);
        return new Uint8Array(await response.arrayBuffer());
    },
    writeFile: (name, relativePath, contents) =>
        request('PUT', `${project(name)}/files/${encodePath(relativePath)}`, {body: contents}),
    makeDirectory: (name, relativePath) =>
        request('PUT', `${project(name)}/files/${encodePath(relativePath)}?directory=1`),
    trashFile: (name, relativePath) => request('DELETE', `${project(name)}/files/${encodePath(relativePath)}`),
    moveFile: (name, from, to) => request('POST', `${project(name)}/move`, {json: {from, to}}),
    describeScene: (name, scenePath) =>
        request('GET', `${project(name)}/scene?path=${encodeURIComponent(scenePath)}`),

    /** Resolves to {ok: true, sb3, blocks, elapsed} or {ok: false, error}. */
    async build (name, source) {
        const response = await request('POST', `${project(name)}/build`, {json: {source}, raw: true});
        if (response.status === 422) {
            const body = await response.json();
            return {ok: false, error: body.error, elapsed: body.elapsed};
        }
        if (!response.ok) {
            let body = null;
            try {
                body = await response.json();
            } catch {
                // Not JSON.
            }
            throw new RakeApiError(response.status, body);
        }
        return {
            ok: true,
            sb3: await response.arrayBuffer(),
            blocks: Number(response.headers.get('X-Rake-Blocks') || 0),
            elapsed: Number(response.headers.get('X-Rake-Elapsed') || 0)
        };
    },
    downloadUrl: (name, source) =>
        `${project(name)}/build?source=${encodeURIComponent(source)}&download=1`
};
