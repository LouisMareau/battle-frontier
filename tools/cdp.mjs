// A small Chrome DevTools Protocol client for the checks (shared with the author's other tools); `args` adds Chrome flags.
// Drives a headless Chrome through the DevTools protocol, with nothing to install:
// Node's own WebSocket and a Chrome (or Edge) that's already on the machine.
// The browser runs with a throwaway profile, apart from the user's own.
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inflateSync } from 'node:zlib';

const CANDIDATES = [
  process.env.CHROME,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
];

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export class Browser {
  /// `profiles`: where the throwaway profile goes. A disk with room: on a nearly full one,
  /// Chrome fails to hold large files (a ROM made into a Blob) in ways that look like network errors.
  static async launch({ width = 1400, height = 900, port = 9333, headless = true, profiles = tmpdir(), args: extra = [] } = {}) {
    const binary = CANDIDATES.find((path) => path && existsSync(path));
    if (!binary) throw new Error('No Chrome found: set CHROME to its path');
    mkdirSync(profiles, { recursive: true });
    const profile = mkdtempSync(join(profiles, 'gameengine-chrome-'));
    const args = [`--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, `--window-size=${width},${height}`, '--no-first-run', '--no-default-browser-check',
      '--enable-unsafe-swiftshader', '--force-device-scale-factor=1',
      // never a sound out of a check: the game plays real audio, from a browser nobody sees
      '--mute-audio', ...extra, 'about:blank'];
    if (headless) args.unshift('--headless=new');
    const child = spawn(binary, args, { stdio: 'ignore' });
    const browser = new Browser(child, profile, port);
    // a browser that quits fails what's waiting on it, instead of leaving the check hanging
    child.on('exit', (code, signal) => browser.lost(`Chrome quit (${signal ?? `exit code ${code}`})`));
    try {
      await browser.connect();
    } catch (error) {
      await browser.close();
      throw error;
    }
    return browser;
  }

  constructor(child, profile, port) {
    Object.assign(this, { child, profile, port, id: 0, waiting: new Map(), listeners: new Map(), console: [], errors: [], mouse: { x: 0, y: 0 } });
  }

  async connect() {
    let target;
    for (let i = 0; i < 100 && !target; i++) {
      await sleep(100);
      try {
        const targets = await (await fetch(`http://127.0.0.1:${this.port}/json/list`)).json();
        target = targets.find((t) => t.type === 'page');
      } catch { /* not up yet */ }
    }
    if (!target) throw new Error('Chrome did not start');
    this.socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { this.socket.onopen = resolve; this.socket.onerror = () => reject(new Error('no connection to Chrome')); });
    this.socket.onclose = () => this.lost('Chrome closed the connection (the page or the browser crashed?)');
    this.socket.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const pending = this.waiting.get(message.id);
        if (!pending) return; // given up on already
        const { resolve, reject } = pending;
        this.waiting.delete(message.id);
        message.error ? reject(new Error(message.error.message)) : resolve(message.result);
      } else if (this.listeners.has(message.method)) {
        this.listeners.get(message.method)(message.params);
        this.listeners.delete(message.method);
      } else if (message.method === 'Runtime.consoleAPICalled') {
        this.console.push(`${message.params.type}: ${message.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
      } else if (message.method === 'Runtime.exceptionThrown') {
        this.errors.push(message.params.exceptionDetails.exception?.description ?? message.params.exceptionDetails.text);
      }
    };
    await this.send('Page.enable');
    await this.send('Runtime.enable');
  }

  /// Chrome is gone: everything waiting on it fails with why, and so does what's asked next.
  lost(why) {
    if (this.gone) return;
    this.gone = why;
    for (const { reject } of this.waiting.values()) reject(new Error(why));
    this.waiting.clear();
  }

  /// A DevTools call; it fails if Chrome is gone or doesn't answer within `timeout` ms.
  send(method, params = {}, timeout = 120000) {
    if (this.gone) return Promise.reject(new Error(`${method}: ${this.gone}`));
    const id = ++this.id;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id);
        reject(new Error(`Chrome didn't answer ${method} in ${timeout / 1000} s`));
      }, timeout);
      const done = (f) => (v) => { clearTimeout(timer); f(v); };
      this.waiting.set(id, { resolve: done(resolve), reject: done(reject) });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  /// The next event of a kind (its parameters)
  once(method, timeout = 15000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`No ${method} event`)), timeout);
      this.listeners.set(method, (params) => { clearTimeout(timer); resolve(params); });
    });
  }

  /// The page's size, whatever the window's
  async viewport(width, height) {
    await this.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
  }

  /// Clicks what opens a file chooser, and gives it files instead of showing it
  async choose(files, x, y) {
    await this.send('Page.setInterceptFileChooserDialog', { enabled: true });
    const opened = this.once('Page.fileChooserOpened');
    await this.click(x, y);
    const { backendNodeId } = await opened;
    await this.send('DOM.setFileInputFiles', { files, backendNodeId });
    await this.send('Page.setInterceptFileChooserDialog', { enabled: false });
  }

  async goto(url) {
    await this.send('Page.navigate', { url });
  }

  /// The value of a JavaScript expression in the page (promises are awaited)
  async evaluate(expression) {
    const { result, exceptionDetails } = await this.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    return result.value;
  }

  async waitFor(expression, { timeout = 30000, what = expression } = {}) {
    const end = Date.now() + timeout;
    while (Date.now() < end) {
      if (await this.evaluate(expression).catch(() => false)) return;
      if (this.gone) throw new Error(`Waiting for ${what}: ${this.gone}`);
      await sleep(100);
    }
    throw new Error(`Timed out waiting for ${what}`);
  }

  wait(ms) { return sleep(ms); }

  async screenshot(path) {
    const { data } = await this.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(path, Buffer.from(data, 'base64'));
    return path;
  }

  /// The page as pixels: { width, height, rgba } (a Buffer, 4 bytes a pixel), and saved to
  /// `path` too when given.
  async pixels(path) {
    const { data } = await this.send('Page.captureScreenshot', { format: 'png' });
    const png = Buffer.from(data, 'base64');
    if (path) writeFileSync(path, png);
    return decodePng(png);
  }

  // -- the pointer ---------------------------------------------------------------

  async move(x, y, buttons = 0) {
    this.mouse = { x, y };
    await this.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, buttons, button: buttons === 1 ? 'left' : buttons === 2 ? 'right' : 'none' });
  }

  async press(button = 'left') {
    await this.send('Input.dispatchMouseEvent', { type: 'mousePressed', ...this.mouse, button, buttons: button === 'left' ? 1 : 2, clickCount: 1 });
  }

  /// Types text into whatever has the focus (click a field first).
  async type(text) {
    await this.send('Input.insertText', { text });
    await sleep(80);
  }

  /// Scrolls with the wheel over a point (`dy` pixels; positive: down the page).
  async wheel(x, y, dy) {
    this.mouse = { x, y };
    await this.send('Input.dispatchMouseEvent', { type: 'mouseWheel', x, y, deltaX: 0, deltaY: dy });
  }

  async release(button = 'left') {
    await this.send('Input.dispatchMouseEvent', { type: 'mouseReleased', ...this.mouse, button, buttons: 0, clickCount: 1 });
  }

  async click(x, y, button = 'left') {
    await this.move(x, y);
    await sleep(60);
    await this.press(button);
    await sleep(60);
    await this.release(button);
    await sleep(120);
  }

  /// Two quick clicks in one place.
  async dblclick(x, y) {
    await this.move(x, y);
    await sleep(60);
    for (const count of [1, 2]) {
      await this.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: count });
      await sleep(40);
      await this.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: count });
      await sleep(60);
    }
    await sleep(120);
  }

  /// A drag through the given points, the button held
  async drag(points, button = 'left') {
    const buttons = button === 'left' ? 1 : 2;
    await this.move(...points[0]);
    await sleep(60);
    await this.press(button);
    for (const [x, y] of points.slice(1)) {
      await sleep(40);
      await this.move(x, y, buttons);
    }
    await sleep(60);
    await this.release(button);
    await sleep(120);
  }

  // a key tapped (or held `hold` ms before it's let go: an arrow walking on)
  async key(key, { ctrl = false, shift = false, hold = 0 } = {}) {
    const modifiers = (ctrl ? 2 : 0) | (shift ? 8 : 0);
    const signs = { '=': ['Equal', 187], '-': ['Minus', 189] };
    const [code, virtualKey] = signs[key] ?? (key.length === 1 ? [`Key${key.toUpperCase()}`, key.toUpperCase().charCodeAt(0)] : [key, 0]);
    const params = { key, code, modifiers, windowsVirtualKeyCode: virtualKey };
    await this.send('Input.dispatchKeyEvent', { type: 'keyDown', ...params });
    if (hold > 0) await sleep(hold);
    await this.send('Input.dispatchKeyEvent', { type: 'keyUp', ...params });
    await sleep(120);
  }

  async type(text) {
    await this.send('Input.insertText', { text });
    await sleep(120);
  }

  /// Files dragged from the system and dropped at a point of the page
  async drop(files, x, y) {
    const data = { items: [], files, dragOperationsMask: 1 };
    for (const type of ['dragEnter', 'dragOver', 'drop']) {
      await this.send('Input.dispatchDragEvent', { type, x, y, data });
      await sleep(80);
    }
  }

  /// Downloads go to a folder instead of asking
  async downloadsTo(downloadPath) {
    await this.send('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath });
  }

  async close() {
    try { this.socket?.close(); } catch { /* already closed */ }
    this.child.kill();
    await sleep(400);
    try { rmSync(this.profile, { recursive: true, force: true }); } catch { /* Chrome may still hold a file: the system's temp cleanup gets it */ }
  }
}

/// An 8-bit, non-interlaced RGB or RGBA PNG (what Chrome's screenshots are) as RGBA pixels.
export function decodePng(png) {
  let pos = 8;
  let width = 0, height = 0, type = 0;
  const idat = [];
  while (pos < png.length) {
    const len = png.readUInt32BE(pos);
    const kind = png.toString('ascii', pos + 4, pos + 8);
    const body = png.subarray(pos + 8, pos + 8 + len);
    if (kind === 'IHDR') {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      if (body[8] !== 8 || body[12] !== 0) throw new Error('only 8-bit, non-interlaced PNGs');
      type = body[9];
    } else if (kind === 'IDAT') idat.push(body);
    else if (kind === 'IEND') break;
    pos += 12 + len;
  }
  const channels = type === 6 ? 4 : type === 2 ? 3 : 0;
  if (!channels) throw new Error(`PNG colour type ${type}: only RGB or RGBA`);
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const out = Buffer.alloc(width * height * 4);
  let prev = Buffer.alloc(stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? line[i - channels] : 0;
      const b = prev[i];
      const c = i >= channels ? prev[i - channels] : 0;
      let v = line[i];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      line[i] = v & 255;
    }
    for (let x = 0; x < width; x++) {
      for (let k = 0; k < 4; k++) out[(y * width + x) * 4 + k] = k < channels ? line[x * channels + k] : 255;
    }
    prev = line;
  }
  return { width, height, rgba: out };
}
