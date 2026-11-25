import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {after, before, test} from 'node:test';
import {fileURLToPath} from 'node:url';
import WebSocket from 'ws';

import {createServer} from '../server.mjs';
import {LspFrameReader, frameLspMessage} from '../lib/lsp-bridge.mjs';
import {resolveInside, validateProjectName} from '../lib/projects.mjs';
import {parseCompilerError} from '../lib/toolchain.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
let base;
let temp;
let server;

before(async () => {
    temp = await fs.mkdtemp(path.join(os.tmpdir(), 'rake-ide-'));
    const gui = path.join(temp, 'gui');
    await fs.mkdir(gui);
    await fs.writeFile(path.join(gui, 'editor.html'), '<!doctype html><title>IDE</title>');
    const created = createServer({
        projectsDirectory: path.join(temp, 'projects'),
        templatesDirectory: path.join(here, '..', '..', 'templates'),
        guiDirectory: gui,
        log: () => {}
    });
    await created.init();
    server = created.server;
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    base = `http://127.0.0.1:${server.address().port}`;
});

after(async () => {
    await new Promise(resolve => server.close(resolve));
    await fs.rm(temp, {recursive: true, force: true});
});

const json = async (method, url, body) => {
    const response = await fetch(`${base}${url}`, {
        method,
        headers: body ? {'Content-Type': 'application/json'} : {},
        body: body ? JSON.stringify(body) : undefined
    });
    return {status: response.status, body: await response.json()};
};

test('validates names and keeps paths inside the project', () => {
    assert.equal(validateProjectName(' Game 1 '), 'Game 1');
    assert.throws(() => validateProjectName('../x'));
    assert.throws(() => validateProjectName('a:b'));
    assert.throws(() => validateProjectName('.hidden'));
    const root = path.resolve('/project');
    assert.equal(resolveInside(root, 'src/a.rk').relative, 'src/a.rk');
    assert.throws(() => resolveInside(root, '../escape.rk'));
    assert.throws(() => resolveInside(root, 'src/../../escape.rk'));
    assert.throws(() => resolveInside(root, '.trash/x'));
    assert.throws(() => resolveInside(root, 'C:/Windows/win.ini'));
});

test('parses compiler JSON errors into project-relative diagnostics', () => {
    const root = path.resolve('/project');
    const line = JSON.stringify({file: path.join(root, 'src', 'a.rk'), line: 3, column: 4, message: 'bad'});
    assert.deepEqual(parseCompilerError(`${line}\n`, root), {file: 'src/a.rk', line: 3, column: 4, message: 'bad'});
    assert.equal(parseCompilerError('plain failure', root).message, 'plain failure');
});

test('splits LSP frames across chunk boundaries', () => {
    const messages = [];
    const reader = new LspFrameReader(text => messages.push(JSON.parse(text)));
    const frame = Buffer.concat([frameLspMessage('{"id":1,"text":"日本語"}'), frameLspMessage('{"id":2}')]);
    reader.push(frame.subarray(0, 10));
    reader.push(frame.subarray(10, 40));
    reader.push(frame.subarray(40));
    assert.deepEqual(messages, [{id: 1, text: '日本語'}, {id: 2}]);
});

test('manages projects and files', async () => {
    let result = await json('POST', '/api/projects', {name: 'demo'});
    assert.equal(result.status, 201);
    assert.equal(result.body.manifest.mainScene, 'scenes/main.rks');
    assert.ok(result.body.files.some(file => file.path === 'src/player.rk'));

    assert.equal((await json('POST', '/api/projects', {name: 'demo'})).status, 409);

    let response = await fetch(`${base}/api/projects/demo/files/src/new.rk`, {method: 'PUT', body: 'var x: int = 1'});
    assert.equal(response.status, 200);
    response = await fetch(`${base}/api/projects/demo/files/src/new.rk`);
    assert.equal(await response.text(), 'var x: int = 1');
    response = await fetch(`${base}/api/projects/demo/files/..%2F..%2Fescape.rk`, {method: 'PUT', body: 'x'});
    assert.equal(response.status, 400);

    result = await json('POST', '/api/projects/demo/move', {from: 'src/new.rk', to: 'src/renamed.rk'});
    assert.equal(result.body.path, 'src/renamed.rk');
    result = await json('DELETE', '/api/projects/demo/files/src/renamed.rk');
    assert.match(result.body.trashedTo, /\.trash/);

    result = await json('POST', '/api/projects/demo/duplicate', {name: 'copy'});
    assert.equal(result.body.manifest.name, 'copy');
    result = await json('POST', '/api/projects/copy/rename', {name: 'renamed'});
    assert.equal(result.body.name, 'renamed');
    result = await json('DELETE', '/api/projects/renamed');
    assert.equal(result.status, 200);
    const names = (await json('GET', '/api/projects')).body.map(project => project.name);
    assert.deepEqual(names, ['demo']);

    // Changes requested by another site are refused.
    response = await fetch(`${base}/api/projects/demo/files/src/x.rk`, {
        method: 'PUT', body: 'x', headers: {Origin: 'https://example.com'}
    });
    assert.equal(response.status, 403);
});

test('builds scenes and reports compiler errors', async () => {
    await json('POST', '/api/projects', {name: 'build'});
    let response = await fetch(`${base}/api/projects/build/build`, {
        method: 'POST', body: JSON.stringify({source: 'scenes/main.rks'})
    });
    assert.equal(response.status, 200);
    const sb3 = Buffer.from(await response.arrayBuffer());
    assert.equal(sb3.subarray(0, 2).toString(), 'PK');
    assert.ok(Number(response.headers.get('x-rake-blocks')) > 0);

    const scene = await json('GET', '/api/projects/build/scene?path=scenes/main.rks');
    assert.equal(scene.body.sprites[0].name, 'Player');
    assert.deepEqual(scene.body.sprites[0].scripts, ['../src/player.rk']);

    await fetch(`${base}/api/projects/build/files/src/player.rk`, {method: 'PUT', body: 'var x: int = true'});
    response = await fetch(`${base}/api/projects/build/build`, {
        method: 'POST', body: JSON.stringify({source: 'scenes/main.rks'})
    });
    assert.equal(response.status, 422);
    const {error} = await response.json();
    assert.equal(error.file, 'src/player.rk');
    assert.equal(error.line, 1);
});

test('bridges the language server over WebSocket', async () => {
    const socket = new WebSocket(`${base.replace('http', 'ws')}/lsp`);
    await new Promise((resolve, reject) => {
        socket.once('open', resolve);
        socket.once('error', reject);
    });
    const received = [];
    socket.on('message', data => received.push(JSON.parse(data.toString())));
    const waitFor = predicate => new Promise(resolve => {
        const check = () => {
            const found = received.find(predicate);
            if (found) resolve(found);
            else setTimeout(check, 20);
        };
        check();
    });
    socket.send(JSON.stringify({jsonrpc: '2.0', id: 1, method: 'initialize', params: {capabilities: {}}}));
    const init = await waitFor(message => message.id === 1);
    assert.ok(init.result.capabilities.completionProvider);
    socket.send(JSON.stringify({
        jsonrpc: '2.0',
        method: 'textDocument/didOpen',
        params: {textDocument: {
            uri: 'file:///x/main.rks', languageId: 'rake-scene', version: 1, text: 'scene A { sprite P { size: "x" } }'
        }}
    }));
    const diagnostics = await waitFor(message => message.method === 'textDocument/publishDiagnostics');
    assert.match(diagnostics.params.diagnostics[0].message, /`size` expects a number/);
    socket.close();
});
