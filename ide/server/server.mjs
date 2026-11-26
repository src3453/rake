// Rake IDE backend: serves the TurboWarp-based editor, project files, live builds,
// and a WebSocket bridge to the Rake language server. It listens on localhost only.

import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {WebSocketServer} from 'ws';

import {attachLanguageServer} from './lib/lsp-bridge.mjs';
import {ProjectError, ProjectStore, resolveInside} from './lib/projects.mjs';
import {Toolchain} from './lib/toolchain.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(here, '../..');

const MIME_TYPES = {
    '.html': 'text/html; charset=utf-8',
    '.js': 'text/javascript; charset=utf-8',
    '.mjs': 'text/javascript; charset=utf-8',
    '.css': 'text/css; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.svg': 'image/svg+xml',
    '.png': 'image/png',
    '.jpg': 'image/jpeg',
    '.jpeg': 'image/jpeg',
    '.gif': 'image/gif',
    '.bmp': 'image/bmp',
    '.ico': 'image/x-icon',
    '.wav': 'audio/wav',
    '.mp3': 'audio/mpeg',
    '.woff': 'font/woff',
    '.woff2': 'font/woff2',
    '.ttf': 'font/ttf',
    '.map': 'application/json; charset=utf-8',
    '.wasm': 'application/wasm',
    '.rk': 'text/plain; charset=utf-8',
    '.rks': 'text/plain; charset=utf-8',
    '.sb3': 'application/x.scratch.sb3'
};

const MAX_BODY = 64 * 1024 * 1024;

const sendJson = (response, status, value) => {
    const body = JSON.stringify(value);
    response.writeHead(status, {'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store'});
    response.end(body);
};

const readBody = request => new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    request.on('data', chunk => {
        size += chunk.length;
        if (size > MAX_BODY) {
            reject(new ProjectError(413, 'Request body is too large'));
            request.destroy();
            return;
        }
        chunks.push(chunk);
    });
    request.on('end', () => resolve(Buffer.concat(chunks)));
    request.on('error', reject);
});

const readJson = async request => {
    const body = await readBody(request);
    if (!body.length) return {};
    try {
        return JSON.parse(body.toString('utf8'));
    } catch {
        throw new ProjectError(400, 'Request body must be JSON');
    }
};

const serveFile = (response, file, extraHeaders = {}) => {
    fs.stat(file, (error, stat) => {
        if (error || !stat.isFile()) {
            sendJson(response, 404, {error: 'Not found'});
            return;
        }
        response.writeHead(200, {
            'Content-Type': MIME_TYPES[path.extname(file).toLowerCase()] || 'application/octet-stream',
            'Content-Length': stat.size,
            ...extraHeaders
        });
        fs.createReadStream(file).pipe(response);
    });
};

export const createServer = ({
    projectsDirectory,
    templatesDirectory = path.join(here, '..', 'templates'),
    guiDirectory = path.join(here, '..', 'gui', 'scratch-gui', 'build'),
    toolchain = new Toolchain(repositoryRoot, {rakeBin: process.env.RAKE_BIN, lspBin: process.env.RAKE_LSP_BIN}),
    log = console.log
}) => {
    const store = new ProjectStore(projectsDirectory, templatesDirectory);
    const monacoDirectory = path.join(here, 'node_modules', 'monaco-editor', 'min');

    // Requests that change files must come from the IDE page itself, not from other sites.
    const sameOrigin = request => {
        const origin = request.headers.origin;
        if (!origin) return true;
        try {
            return new URL(origin).host === request.headers.host;
        } catch {
            return false;
        }
    };

    const api = async (request, response, url) => {
        const parts = url.pathname.split('/').slice(2)
            .map(decodeURIComponent);
        const method = request.method;
        if (method !== 'GET' && method !== 'HEAD' && !sameOrigin(request)) {
            throw new ProjectError(403, 'Cross-origin requests are not allowed');
        }

        if (parts[0] === 'info' && method === 'GET') {
            let binaries = {};
            try {
                binaries = {rake: toolchain.rake, lsp: toolchain.lsp};
            } catch (error) {
                binaries = {error: error.message};
            }
            return sendJson(response, 200, {
                projectsDirectory: store.root,
                templates: await store.templateNames(),
                binaries
            });
        }
        if (parts[0] === 'import' && parts.length === 1 && method === 'POST') {
            // Experimental: the body is an .sb3 archive that becomes a new project.
            const name = url.searchParams.get('name');
            const result = await store.importSb3(name, await readBody(request),
                (sb3Path, directory) => toolchain.decompile(sb3Path, directory));
            return sendJson(response, 201, result);
        }
        if (parts[0] !== 'projects') throw new ProjectError(404, 'Unknown API endpoint');

        if (parts.length === 1) {
            if (method === 'GET') return sendJson(response, 200, await store.list());
            if (method === 'POST') {
                const {name, template} = await readJson(request);
                return sendJson(response, 201, await store.create(name, template));
            }
        }
        const name = parts[1];
        if (parts.length === 2) {
            if (method === 'GET') return sendJson(response, 200, await store.describe(name));
            if (method === 'DELETE') return sendJson(response, 200, await store.trash(name));
        }
        const action = parts[2];
        if (action === 'rename' && method === 'POST') {
            const body = await readJson(request);
            return sendJson(response, 200, await store.rename(name, body.name));
        }
        if (action === 'duplicate' && method === 'POST') {
            const body = await readJson(request);
            return sendJson(response, 201, await store.duplicate(name, body.name));
        }
        if (action === 'manifest' && method === 'PUT') {
            return sendJson(response, 200, await store.writeManifest(name, await readJson(request)));
        }
        if (action === 'files') {
            const relative = parts.slice(3).join('/');
            if (method === 'GET') {
                const contents = await store.readFile(name, relative);
                response.writeHead(200, {
                    'Content-Type': MIME_TYPES[path.extname(relative).toLowerCase()] || 'application/octet-stream',
                    'Cache-Control': 'no-store'
                });
                return response.end(contents);
            }
            if (method === 'PUT') {
                if (url.searchParams.get('directory') === '1') {
                    return sendJson(response, 200, await store.makeDirectory(name, relative));
                }
                return sendJson(response, 200, await store.writeFile(name, relative, await readBody(request)));
            }
            if (method === 'DELETE') return sendJson(response, 200, await store.trashFile(name, relative));
        }
        if (action === 'move' && method === 'POST') {
            const {from, to} = await readJson(request);
            return sendJson(response, 200, await store.moveFile(name, from, to));
        }
        if (action === 'build' && (method === 'POST' || method === 'GET')) {
            const root = await store.requireProject(name);
            const source = method === 'POST' ? (await readJson(request)).source : url.searchParams.get('source');
            const {absolute, relative} = resolveInside(root, source || '');
            if (!/\.rks?$/.test(relative)) throw new ProjectError(400, 'Build a .rks scene or a .rk source');
            const result = await toolchain.build(root, absolute);
            if (!result.ok) return sendJson(response, 422, {error: result.error, elapsed: result.elapsed});
            const fileName = `${name}-${path.basename(relative).replace(/\.[^.]+$/, '')}.sb3`;
            response.writeHead(200, {
                'Content-Type': 'application/x.scratch.sb3',
                'Content-Length': result.sb3.length,
                'Cache-Control': 'no-store',
                'X-Rake-Blocks': String(result.blocks),
                'X-Rake-Elapsed': String(result.elapsed),
                ...(url.searchParams.get('download') === '1' ? {
                    'Content-Disposition': `attachment; filename*=UTF-8''${encodeURIComponent(fileName)}`
                } : {})
            });
            return response.end(result.sb3);
        }
        if (action === 'scene' && method === 'GET') {
            const root = await store.requireProject(name);
            const {absolute} = resolveInside(root, url.searchParams.get('path') || '');
            const result = await toolchain.describeScene(root, absolute);
            return sendJson(response, result.ok ? 200 : 422, result.ok ? result.scene : {error: result.error});
        }
        throw new ProjectError(404, 'Unknown API endpoint');
    };

    const server = http.createServer(async (request, response) => {
        const url = new URL(request.url, `http://${request.headers.host || 'localhost'}`);
        try {
            if (url.pathname.startsWith('/api/')) {
                await api(request, response, url);
                return;
            }
            if (url.pathname.startsWith('/monaco/')) {
                const file = path.join(monacoDirectory, path.normalize(decodeURIComponent(url.pathname.slice('/monaco/'.length))));
                if (!file.startsWith(monacoDirectory)) throw new ProjectError(403, 'Forbidden');
                serveFile(response, file, {'Cache-Control': 'max-age=86400'});
                return;
            }
            const requested = url.pathname === '/' ? '/editor.html' : decodeURIComponent(url.pathname);
            const file = path.join(guiDirectory, path.normalize(requested));
            if (!file.startsWith(guiDirectory)) throw new ProjectError(403, 'Forbidden');
            if (!fs.existsSync(guiDirectory)) {
                response.writeHead(503, {'Content-Type': 'text/plain; charset=utf-8'});
                response.end(`The IDE has not been built yet. Run \`npm run setup\` in ide/ (missing ${guiDirectory}).`);
                return;
            }
            serveFile(response, file);
        } catch (error) {
            const status = error.status || 500;
            if (status === 500) log(error);
            if (!response.headersSent) sendJson(response, status, {error: {message: error.message}});
        }
    });

    const sockets = new WebSocketServer({noServer: true});
    server.on('upgrade', (request, socket, head) => {
        const url = new URL(request.url, 'http://localhost');
        if (url.pathname !== '/lsp' || !sameOrigin(request)) {
            socket.destroy();
            return;
        }
        sockets.handleUpgrade(request, socket, head, ws => {
            try {
                attachLanguageServer(ws, toolchain.lsp, log);
            } catch (error) {
                log(error.message);
                ws.close(1011, 'language server unavailable');
            }
        });
    });

    return {server, store, init: () => store.init()};
};

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
    const port = Number(process.env.PORT || 8610);
    const projectsDirectory = process.env.RAKE_PROJECTS_DIR || path.join(os.homedir(), 'RakeProjects');
    const {server, store, init} = createServer({projectsDirectory});
    await init();
    if (!(await store.list()).length) {
        await store.create('sample');
        console.log(`Created a sample project in ${path.join(store.root, 'sample')}`);
    }
    server.listen(port, '127.0.0.1', () => {
        console.log(`Rake IDE: http://localhost:${port}/`);
        console.log(`Projects: ${store.root}`);
    });
}
