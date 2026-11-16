const test = require('node:test');
const assert = require('node:assert/strict');
const {setup, deferred, flush} = require('./helpers.cjs');

test('auth precedes requests; no page grant by default', async () => {
  const h = setup(), s = await h.connected();
  assert.equal(s.url, 'ws://127.0.0.1:45001');
  assert.equal(s.messages[0].type, 'auth');
  assert.equal(s.messages[0].clientId, 'a'.repeat(32));
  assert.match(s.messages[0].proof, /^[a-f0-9]{64}$/);
  assert.equal(s.messages[0].token, undefined);
  assert.match((await h.command(s, 'snapshot')).error, /No tab enabled/);
  assert.equal(h.pairs.length, 0);
});
test('status separates connection from grant without querying a page', async () => {
  const h = setup(), s = await h.connected();
  h.browser.tabs.get = () => {throw new Error('status must not access tabs');};
  const state = (await h.command(s, 'status')).result;
  assert.equal(state.connected, true);
  assert.equal(state.tabEnabled, false);
  assert.equal(state.tabId, null);
  assert.match(state.nextStep, /Enable current tab/);
  assert.equal(h.pairs.length, 0);
});
test('independent revoke keeps transport alive, denies reads/actions and fences old snapshots', async () => {
  const h = setup(), s = await h.connected();
  await h.send({kind: 'enable'});
  const snapshot = (await h.command(s, 'snapshot')).result;
  assert.equal((await h.command(s, 'status')).result.tabEnabled, true);
  await h.send({kind: 'revoke'});
  const state = (await h.command(s, 'status')).result;
  assert.equal(state.connected, true);
  assert.equal(state.tabEnabled, false);
  assert.notEqual(s.closed, true);
  assert.match((await h.command(s, 'snapshot')).error, /No tab enabled/);
  assert.match((await h.command(s, 'click', {snapshotId:snapshot.snapshotId,elementId:1})).error, /No tab enabled/);
  await h.send({kind: 'enable'});
  assert.match((await h.command(s, 'click', {snapshotId:snapshot.snapshotId,elementId:1})).error, /Stale snapshot/);
  assert.equal(h.content.button.clicks, 0);
  assert.ok((await h.command(s, 'snapshot')).result.snapshotId);
});
test('independent revoke fences an action waiting for tab metadata', async () => {
  const h = setup(), s = await h.connected();
  await h.send({kind: 'enable'});
  const snapshot = (await h.command(s, 'snapshot')).result;
  const gate = deferred(), original = h.browser.tabs.get;
  h.browser.tabs.get = () => gate.promise;
  const command = h.command(s, 'click', {snapshotId:snapshot.snapshotId,elementId:1});
  await h.send({kind: 'revoke'});
  gate.resolve(await original());
  assert.match((await command).error, /revoked/);
  assert.equal(h.content.button.clicks, 0);
  assert.notEqual(s.closed, true);
});
test('only own popup may grant or configure', async () => {
  const h = setup();
  await h.events.message.emit({kind: 'connect', token: 'a'.repeat(64)}, {id: 'extension', url: 'https://example.org/'})[0];
  assert.equal(h.sockets.length, 0);
});
test('explicit grant handshakes before routing; navigation revokes it', async () => {
  const h = setup(), s = await h.connected();
  await h.send({kind: 'enable'});
  const snapshot = (await h.command(s, 'snapshot')).result;
  assert.equal(snapshot.elements.length, 2);
  assert.ok(snapshot.context.grantId);
  h.events.updated.emit(10, {status: 'loading'});
  assert.equal((await h.send({kind: 'status'})).tabId, null);
  assert.equal(h.pairs[0].background.closed, true);
});
test('disconnect while tabs.get is pending prevents dispatch', async () => {
  const h = setup(), s = await h.connected();
  await h.send({kind: 'enable'});
  const snapshot = (await h.command(s, 'snapshot')).result;
  const gate = deferred(), original = h.browser.tabs.get;
  h.browser.tabs.get = () => gate.promise;
  const command = h.command(s, 'click', {snapshotId: snapshot.snapshotId, elementId: 1});
  await h.send({kind: 'disconnect'});
  gate.resolve(await original()); await command;
  assert.equal(h.content.button.clicks, 0);
  assert.equal(h.pairs[0].background.messages.filter(m => m.method === 'click').length, 0);
});
for (const stage of ['query', 'executeScript', 'get']) {
  test(`disconnect during enable ${stage} cannot resurrect permission`, async () => {
    const h = setup(); await h.connected();
    const gate = deferred(), entered = deferred(), original = h.browser.tabs[stage];
    h.browser.tabs[stage] = async (...args) => {entered.resolve(); await gate.promise; return original(...args);};
    const enabling = h.send({kind: 'enable'});
    const rejected = assert.rejects(enabling, /revoked/);
    await entered.promise; await h.send({kind: 'disconnect'}); gate.resolve(); await rejected;
    assert.equal((await h.send({kind: 'status'})).tabId, null);
    assert.equal(h.pairs.length, 0);
  });
}
test('newer enable wins; a delayed old enable cannot revoke it', async () => {
  const h = setup(); await h.connected();
  const gate = deferred(), original = h.browser.tabs.query;
  h.browser.tabs.query = () => gate.promise;
  const old = assert.rejects(h.send({kind: 'enable'}), /revoked/);
  h.browser.tabs.query = original;
  await h.send({kind: 'enable'}); gate.resolve(await original()); await old;
  assert.equal((await h.send({kind: 'status'})).tabId, 10);
});
test('regrant clears snapshots; old port disconnect cannot revoke replacement', async () => {
  const h = setup(), s = await h.connected();
  await h.send({kind: 'enable'});
  const snapshot = (await h.command(s, 'snapshot')).result, previous = h.pairs[0];
  await h.send({kind: 'enable'});
  previous.background.onDisconnect.emit(); previous.content.onDisconnect.emit();
  const result = await h.command(s, 'click', {snapshotId: snapshot.snapshotId, elementId: 1});
  assert.match(result.error, /Stale snapshot/);
  assert.equal(h.content.button.clicks, 0);
  assert.equal((await h.send({kind: 'status'})).tabId, 10);
  assert.notEqual((await h.command(s, 'snapshot')).result.context.grantId, snapshot.context.grantId);
});
test('disconnect during grant handshake rejects enable promptly', async () => {
  const h = setup(); await h.connected();
  h.content.attach = () => {};
  const enabled = assert.rejects(h.send({kind: 'enable'}), /revoked/);
  await flush(); await h.send({kind: 'disconnect'}); await enabled;
  assert.equal((await h.send({kind: 'status'})).tabId, null);
});
test('no duplicate action while an earlier command awaits tabs.get', async () => {
  const h = setup(), s = await h.connected(); await h.send({kind: 'enable'});
  const snapshot = (await h.command(s, 'snapshot')).result;
  const gate = deferred(), original = h.browser.tabs.get;
  h.browser.tabs.get = () => gate.promise;
  const first = h.command(s, 'click', {snapshotId: snapshot.snapshotId, elementId: 1}, 'first');
  const duplicate = await h.command(s, 'click', {snapshotId: snapshot.snapshotId, elementId: 1}, 'duplicate');
  assert.match(duplicate.error, /in flight/);
  gate.resolve(await original()); await first;
  assert.equal(h.content.button.clicks, 1);
});
test('connection loss revokes access; reconnect still requires enable', async () => {
  const h = setup(), s = await h.connected(); await h.send({kind: 'enable'}); s.close();
  assert.equal((await h.send({kind: 'status'})).tabId, null);
  const next = await h.connected();
  assert.match((await h.command(next, 'snapshot')).error, /No tab enabled/);
  s.onerror();
  assert.equal((await h.send({kind: 'status'})).connectionState, 'Connected');
});
test('disconnect during stored-token read cannot reconnect later', async () => {
  const h = setup(), gate = deferred();
  h.browser.storage.local.get = () => gate.promise;
  const connecting = assert.rejects(h.send({kind: 'connect'}), /cancelled/);
  await h.send({kind: 'disconnect'}); gate.resolve({token: 'a'.repeat(64)}); await connecting;
  assert.equal(h.sockets.length, 0);
});
test('pagehide revokes grant and restoration requires explicit enable', async () => {
  const h = setup(), s = await h.connected(); await h.send({kind: 'enable'});
  h.content.fire('pagehide'); await flush(); h.content.fire('pageshow');
  assert.equal((await h.send({kind: 'status'})).tabId, null);
  assert.match((await h.command(s, 'snapshot')).error, /No tab enabled/);
  await h.send({kind: 'enable'});
  assert.ok((await h.command(s, 'snapshot')).result.snapshotId);
});
for (const event of ['navigation', 'closed']) {
  test(`${event} during handshake cannot activate pending grant`, async () => {
    const h = setup(); await h.connected();
    h.content.attach = () => {};
    const enabling = assert.rejects(h.send({kind: 'enable'}), /revoked/);
    await flush();
    if (event === 'navigation') h.events.updated.emit(10, {status: 'loading'});
    else h.events.removed.emit(10);
    await enabling;
    assert.equal((await h.send({kind: 'status'})).tabId, null);
    assert.equal(h.pairs[0].background.closed, true);
  });
}
test('lost page response settles unknown immediately and releases in-flight guard', async () => {
  const h = setup(), s = await h.connected(); await h.send({kind: 'enable'});
  const snapshot = (await h.command(s, 'snapshot')).result;
  const pagePort = h.pairs[0].content;
  pagePort.postMessage = () => pagePort.disconnect();
  const reply = await h.command(s, 'click', {snapshotId: snapshot.snapshotId, elementId: 1});
  assert.match(reply.error, /outcome may be unknown/);
  assert.equal(h.content.button.clicks, 1);
  assert.equal((await h.send({kind: 'status'})).tabId, null);
  assert.match((await h.command(s, 'snapshot')).error, /No tab enabled/);
  assert.equal(s.closed, undefined);
});
test('page command timeout revokes access and closes only its connection', async () => {
  const h = setup(), s = await h.connected(); await h.send({kind: 'enable'});
  h.pairs[0].content.postMessage = () => {};
  const command = h.command(s, 'snapshot');
  await flush();
  [...h.timers.values()].find(timer => timer.ms === 14000).fn();
  await command;
  assert.equal(s.closed, true);
  assert.equal((await h.send({kind: 'status'})).tabId, null);
});

const A = 'a'.repeat(32), B = 'b'.repeat(32);
test('two chats own distinct tabs; selection, revoke and disconnect cannot cross-route', async () => {
  const h = setup(), a = await h.connected(), b = await h.connected(`45002:${B}`);
  await h.send({kind:'enable',clientId:A});
  const sa = (await h.command(a,'snapshot')).result;
  await assert.rejects(h.send({kind:'enable',clientId:B}),/another chat/);
  assert.equal((await h.command(a,'status')).result.tabEnabled,true);
  h.setTab({id:20,url:'https://other.example/',incognito:false,status:'complete'});
  await h.send({kind:'enable',clientId:B});
  const sb = (await h.command(b,'snapshot')).result;
  assert.equal(sa.context.clientId,A); assert.equal(sb.context.clientId,B);
  assert.match((await h.command(b,'click',{snapshotId:sa.snapshotId,elementId:1})).error,/Stale snapshot/);
  assert.match((await h.command(a,'click',{snapshotId:sb.snapshotId,elementId:1})).error,/Stale snapshot/);
  await h.send({kind:'select',clientId:B});
  await h.command(a,'click',{snapshotId:sa.snapshotId,elementId:1});
  assert.equal(h.content.button.clicks,1); assert.equal(h.pages.get(20).button.clicks,0);
  await h.send({kind:'revoke',clientId:A});
  assert.match((await h.command(a,'snapshot')).error,/No tab enabled/);
  await h.command(b,'click',{snapshotId:sb.snapshotId,elementId:1});
  assert.equal(h.pages.get(20).button.clicks,1);
  await h.send({kind:'disconnect',clientId:A});
  assert.equal((await h.command(b,'status')).result.tabEnabled,true);
  assert.notEqual(b.closed,true);
});
test('pending tab reservation prevents a second chat from stealing an in-flight enable', async () => {
  const h=setup(), a=await h.connected(), b=await h.connected(`45002:${B}`);
  const gate=deferred(), entered=deferred(), original=h.browser.tabs.executeScript;
  h.browser.tabs.executeScript=async (...args)=>{entered.resolve();await gate.promise;return original(...args);};
  const enabling=h.send({kind:'enable',clientId:A}); await entered.promise;
  await assert.rejects(h.send({kind:'enable',clientId:B}),/another chat/);
  gate.resolve(); await enabling;
  assert.equal((await h.command(a,'status')).result.tabEnabled,true);
  assert.equal((await h.command(b,'status')).result.tabEnabled,false);
});
test('navigation and connection loss revoke only the owning chat', async () => {
  const h=setup(),a=await h.connected(),b=await h.connected(`45002:${B}`);
  await h.send({kind:'enable',clientId:A});
  h.setTab({id:20,url:'https://other.example/',incognito:false,status:'complete'});
  await h.send({kind:'enable',clientId:B});
  h.events.updated.emit(10,{status:'loading'});
  assert.equal((await h.command(a,'status')).result.tabEnabled,false);
  assert.equal((await h.command(b,'status')).result.tabEnabled,true);
  a.close();
  assert.equal((await h.command(b,'status')).result.tabEnabled,true);
});
test('queued commands wait for asynchronous server proof and fail closed on rejection', async () => {
  const protocol = require('../protocol.js');
  for (const valid of [true, false]) {
    const gate = deferred(), clientId = 'a'.repeat(32), challenge = 'c'.repeat(64);
    const h = setup({protocol: {...protocol, verifyProof: (...args) => gate.promise.then(() => protocol.verifyProof(...args))}});
    await h.send({kind:'connect',token:'a'.repeat(64)});
    const socket = h.sockets[0]; socket.onopen();
    await socket.onmessage({data:JSON.stringify({type:'challenge',version:2,clientId,challenge})});
    const proof = valid ? await protocol.authProof('a'.repeat(64),clientId,challenge,'server') : '0'.repeat(64);
    const authenticating = socket.onmessage({data:JSON.stringify({type:'ready',clientId,proof})});
    const queued = h.command(socket,'status',{},'queued');
    await flush();
    assert.equal(socket.messages.some(message => message.id === 'queued'), false);
    gate.resolve(); await authenticating;
    const reply = await queued;
    if (valid) assert.equal(reply.result.connected, true);
    else { assert.equal(reply, undefined); assert.equal(socket.closed, true); }
    assert.equal(h.pairs.length, 0);
  }
});

test('wrong server identity or proof never authenticates or sends the shared token', async () => {
  for (const mismatch of ['identity','proof']) {
    const h=setup(); await h.send({kind:'connect',token:'a'.repeat(64)});
    const s=h.sockets[0];s.onopen();
    await s.onmessage({data:JSON.stringify({type:'challenge',version:2,clientId:mismatch==='identity'?B:A,challenge:'c'.repeat(64)})});
    if(mismatch==='proof') await s.onmessage({data:JSON.stringify({type:'ready',clientId:A,proof:'0'.repeat(64)})});
    assert.equal(s.closed,true);
    assert.equal((await h.send({kind:'status'})).ready,false);
    assert.ok(s.messages.every(m=>m.token===undefined));
  }
});
