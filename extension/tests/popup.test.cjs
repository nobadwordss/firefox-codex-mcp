const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {setup, flush} = require('./helpers.cjs');

async function popup(background) {
  const elements = new Map();
  let poll;
  function element(selector) {
    if (!elements.has(selector)) {
      const listeners = new Map();
      elements.set(selector, {value:'',textContent:'',disabled:false,
        addEventListener(name, fn) {listeners.set(name, fn);},
        replaceChildren(...children) {this.children = children;},
        fire(name) {return listeners.get(name)?.();}});
    }
    return elements.get(selector);
  }
  vm.runInNewContext(fs.readFileSync(require.resolve('../popup.js'),'utf8'), {
    document:{querySelector:element}, browser:{runtime:{sendMessage:background.send}},
    Option:class {constructor(text,value) {this.text=text;this.value=value;}},
    setInterval(fn) {poll=fn;return 1;},clearInterval() {},addEventListener() {}
  });
  await flush();
  return {element, poll:() => poll()};
}

test('closing the popup preserves the public pairing code but not an unsubmitted token', async () => {
  const h=setup(), first=await popup(h), code=`45001:${'a'.repeat(32)}`;
  first.element('#connection').value=code;
  first.element('#connection').fire('input');
  first.element('#token').value='b'.repeat(64);
  first.element('#token').fire('input');
  await flush();
  const reopened=await popup(h);
  assert.equal(reopened.element('#connection').value,code);
  assert.equal(reopened.element('#token').value,'');
  assert.equal(h.stored.token,undefined);
  assert.equal(h.sockets.length,0);
});

test('a connection error remains visible across automatic status refreshes', async () => {
  const h=setup(), ui=await popup(h);
  await ui.element('#connect').fire('click');
  assert.match(ui.element('#status').textContent,/Paste the connectionCode/);
  await ui.poll();
  assert.match(ui.element('#status').textContent,/Paste the connectionCode/);
  ui.element('#connection').value=`45001:${'a'.repeat(32)}`;
  ui.element('#connection').fire('input');
  await ui.poll();
  assert.match(ui.element('#status').textContent,/No chat selected/);
});

test('submitting a valid connection clears the pairing draft for the next popup', async () => {
  const h=setup(), ui=await popup(h);
  ui.element('#connection').value=`45001:${'a'.repeat(32)}`;
  ui.element('#connection').fire('input');
  ui.element('#token').value='a'.repeat(64);
  await ui.element('#connect').fire('click');
  const reopened=await popup(h);
  assert.equal(reopened.element('#connection').value,'');
  assert.equal(reopened.element('#token').value,'');
  assert.equal(h.sockets.length,1);
});
