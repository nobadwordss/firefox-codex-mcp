const fs = require('node:fs');
const vm = require('node:vm');
const { randomUUID } = require('node:crypto');
const protocol = require('../protocol.js');
const code = name => fs.readFileSync(require.resolve(`../${name}.js`), 'utf8');
function event() {
  const listeners = new Set();
  return {addListener: fn => listeners.add(fn), removeListener: fn => listeners.delete(fn), emit: (...args) => [...listeners].map(fn => fn(...args))};
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {resolve = yes; reject = no;});
  return {promise, resolve, reject};
}
const flush = () => new Promise(resolve => setImmediate(resolve));
function ports() {
  function endpoint() {
    return {name: protocol.PORT_NAME, sender: {id: 'extension'}, onMessage: event(), onDisconnect: event(), messages: [], closed: false};
  }
  const background = endpoint(), content = endpoint();
  for (const [local, remote] of [[background, content], [content, background]]) {
    local.postMessage = message => {
      if (local.closed) throw new Error('Attempt to postMessage on disconnected port');
      local.messages.push(structuredClone(message));
      queueMicrotask(() => {if (!remote.closed) remote.onMessage.emit(structuredClone(message));});
    };
    // Firefox notifies the other endpoint, not the endpoint calling disconnect().
    local.disconnect = () => {
      if (local.closed) return;
      local.closed = true;
      queueMicrotask(() => {remote.closed = true; remote.onDisconnect.emit();});
    };
  }
  return {background, content};
}
function page() {
  let changed, sequence = 0, now = 0, covered = false;
  const events = {}, onConnect = event();
  class Element {
    constructor(tag = 'BUTTON') {this.nodeType = 1; this.tagName = tag; this.textContent = 'Press'; this.isConnected = true; this.attrs = {}; this.events = []; this.clicks = 0; this.rect = {width: 40, height: 20, bottom: 20, right: 40, top: 0, left: 0};}
    set textContent(value) {this.firstChild = {nodeType: 3, nodeValue: value, parentElement: this, parentNode: this, nextSibling: null};}
    get textContent() {return this.firstChild.nodeValue;}
    getBoundingClientRect() {return this.rect;}
    getClientRects() {return [this.rect];}
    getAttribute(name) {return this.attrs[name] ?? null;}
    hasAttribute(name) {return Object.hasOwn(this.attrs,name);}
    matches(selector) {return selector.split(',').some(s => s === ':disabled' ? !!this.disabled : s.toUpperCase() === this.tagName || s === '[inert]' && this.hasAttribute('inert') || s === '[aria-disabled="true"]' && this.attrs['aria-disabled'] === 'true' || s === '[aria-busy="true"]' && this.attrs['aria-busy'] === 'true');}
    closest(selector) {for (let e = this; e; e = e.parentElement) if (e.matches(selector)) return e; return null;}
    contains(element) {for (let e = element; e; e = e.parentElement) if (e === this) return true; return false;}
    click() {this.clicks++;}
    dispatchEvent(event) {this.events.push(event.type);}
  }
  class Input extends Element {
    constructor(type = 'text') {super('INPUT'); this.type = type; this.autocomplete = ''; this._value = 'private field content';}
    set value(value) {this._value = value;}
    get value() {return this._value;}
  }
  class Textarea extends Element {constructor() {super('TEXTAREA');} set value(value) {this._value = value;}}
  const button = new Element(), input = new Input();
  input.rect = {...input.rect, left: 60, right: 100};
  const elements = [button, input, new Input('password'), new Input('file')];
  class Observer {constructor(callback) {changed = callback;} observe() {} takeRecords() {return [];}}
  const context = vm.createContext({
    FirefoxCodexProtocol: protocol,
    document: {title: 'Example', readyState: 'complete', querySelectorAll: selector => selector.startsWith('a[href]') ? elements : [], getElementById: () => null, createRange: () => ({setStart(node) {this.node = node;}, setEnd() {}, getClientRects() {return [this.node.parentElement.rect];}}), elementFromPoint: x => covered ? new Element() : x < 50 ? button : input}, location: {href: 'https://example.org/'},
    innerWidth: 800, innerHeight: 600, MutationObserver: Observer,
    scrollX: 0, scrollY: 0, performance: {now: () => now},
    addEventListener(name, fn) {(events[name] ??= []).push(fn);},
    getComputedStyle: () => ({visibility: 'visible', display: 'block', opacity: '1'}),
    HTMLInputElement: Input, HTMLTextAreaElement: Textarea, Event: class {constructor(type) {this.type = type;}},
    crypto: {randomUUID}, URL, browser: {runtime: {id: 'extension', onConnect}}, window: {scrollBy() {}}
  });
  const inject = () => {vm.runInContext(code('observation'), context); vm.runInContext(code('page'), context); vm.runInContext(code('content'), context);};
  inject();
  function attach(pair) {onConnect.emit(pair.content);}
  async function grant() {
    const pair = ports();
    const ready = deferred();
    let binding;
    pair.background.onMessage.addListener(message => {
      if (message.kind === 'hello') {
        binding = {clientId: randomUUID(), sessionId: randomUUID(), grantId: randomUUID(), documentId: message.documentId};
        pair.background.postMessage({kind: 'grant', context: binding});
      } else if (message.kind === 'ready') ready.resolve();
    });
    attach(pair); await ready.promise;
    return {
      pair, context: binding,
      async send(method, params = {}) {
        const reply = deferred(), id = String(++sequence);
        const receive = message => {if (message.kind === 'response' && message.id === id) {pair.background.onMessage.removeListener(receive); reply.resolve(message);}};
        pair.background.onMessage.addListener(receive);
        pair.background.postMessage({kind: 'command', id, method, params, context: binding});
        return reply.promise;
      }
    };
  }
  return {button, input, changed: () => changed(), advance: ms => {now += ms;}, cover: () => {covered = true;}, fire: name => events[name]?.forEach(fn => fn()), inject, attach, grant, onConnect};
}
function setup(options = {}) {
  const events = {message: event(), removed: event(), updated: event()}, sockets = [], stored = {}, pairs = [], timers = new Map();
  let timerId = 0, tab = {id: 10, url: 'https://example.org/', incognito: false, status: 'complete'};
  const content = page();
  const tabs = new Map([[10,tab]]), pages = new Map([[10,content]]);
  class Socket {
    static OPEN = 1;
    constructor(url) {this.url = url; this.readyState = 1; this.messages = []; sockets.push(this);}
    send(value) {this.messages.push(JSON.parse(value));}
    close() {if (this.closed) return; this.closed = true; this.readyState = 3; this.onclose?.();}
  }
  const browser = {
    runtime: {id: 'extension', getURL: path => `moz-extension://test/${path}`, onMessage: events.message},
    storage: {local: {async get() {return stored;}, async set(value) {Object.assign(stored, value);}}},
    tabs: {
      onRemoved: events.removed, onUpdated: events.updated, async query() {return [tab];}, async get(id = tab.id) {return tabs.get(id);},
      async executeScript(id, details) {if (details.file === 'content.js') pages.get(id).inject();},
      connect(id) {const pair = ports(); pairs.push(pair); queueMicrotask(() => pages.get(id).attach(pair)); return pair.background;}
    }
  };
  vm.runInNewContext(code('background'), {
    browser, FirefoxCodexProtocol: options.protocol ?? protocol, WebSocket: Socket, TextEncoder, crypto: {randomUUID},
    setTimeout(fn, ms) {const id = ++timerId; timers.set(id, {fn, ms}); return id;}, clearTimeout(id) {timers.delete(id);}
  });
  const popup = {id: 'extension', url: 'moz-extension://test/popup.html'};
  const defaultId = 'a'.repeat(32), defaultCode = `45001:${defaultId}`;
  const send = message => events.message.emit({clientId:defaultId, ...(message.kind === 'connect' ? {connectionCode:defaultCode} : {}), ...message}, popup)[0];
  async function connected(code = defaultCode) {
    const {clientId} = protocol.connectionIdentity(code), challenge = 'c'.repeat(64);
    await send({kind: 'connect', connectionCode:code, token: 'a'.repeat(64)});
    const socket = sockets.at(-1); socket.onopen();
    await socket.onmessage({data:JSON.stringify({type:'challenge',version:2,clientId,challenge})});
    await socket.onmessage({data:JSON.stringify({type:'ready',clientId,proof:await protocol.authProof('a'.repeat(64),clientId,challenge,'server')})});
    return socket;
  }
  async function command(socket, method, params = {}, id = String(socket.messages.length)) {
    await socket.onmessage({data: JSON.stringify({id, method, params})});
    return socket.messages.findLast(message => message.id === id);
  }
  return {events, sockets, pairs, timers, stored, browser, content, pages, send, connected, command, setTab(value) {tab = value; tabs.set(tab.id,tab); if (!pages.has(tab.id)) pages.set(tab.id,page());}};
}
module.exports = {event, deferred, flush, ports, page, setup};
