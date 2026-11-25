// Bridges a browser WebSocket (one JSON-RPC message per text frame) to a `rake-lsp`
// process speaking the LSP base protocol (Content-Length framed) over stdio.

import {spawn} from 'node:child_process';

/** Incrementally split `Content-Length` framed messages out of a byte stream. */
export class LspFrameReader {
    constructor (onMessage) {
        this.buffer = Buffer.alloc(0);
        this.onMessage = onMessage;
    }

    push (chunk) {
        this.buffer = Buffer.concat([this.buffer, chunk]);
        for (;;) {
            const headerEnd = this.buffer.indexOf('\r\n\r\n');
            if (headerEnd < 0) return;
            const header = this.buffer.subarray(0, headerEnd).toString('ascii');
            const match = /Content-Length:\s*(\d+)/i.exec(header);
            if (!match) {
                // Drop a malformed header and resynchronize on the next one.
                this.buffer = this.buffer.subarray(headerEnd + 4);
                continue;
            }
            const length = Number(match[1]);
            const start = headerEnd + 4;
            if (this.buffer.length < start + length) return;
            const body = this.buffer.subarray(start, start + length).toString('utf8');
            this.buffer = this.buffer.subarray(start + length);
            this.onMessage(body);
        }
    }
}

export const frameLspMessage = text => {
    const body = Buffer.from(text, 'utf8');
    return Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'ascii'), body]);
};

export const attachLanguageServer = (socket, command, log = () => {}) => {
    const child = spawn(command, [], {windowsHide: true});
    const reader = new LspFrameReader(text => {
        if (socket.readyState === socket.OPEN) socket.send(text);
    });
    child.stdout.on('data', chunk => reader.push(chunk));
    child.stderr.on('data', chunk => log(`[rake-lsp] ${chunk.toString().trim()}`));
    child.on('error', error => {
        log(`[rake-lsp] failed to start: ${error.message}`);
        socket.close(1011, 'language server failed to start');
    });
    child.on('exit', code => {
        if (socket.readyState === socket.OPEN) socket.close(1011, `language server exited (${code})`);
    });
    socket.on('message', data => {
        if (child.stdin.writable) child.stdin.write(frameLspMessage(data.toString()));
    });
    socket.on('close', () => {
        if (child.exitCode === null) {
            if (child.stdin.writable) {
                child.stdin.write(frameLspMessage(JSON.stringify({jsonrpc: '2.0', method: 'exit'})));
            }
            setTimeout(() => child.kill(), 1000).unref();
        }
    });
    return child;
};
