const test = require('node:test');
const assert = require('node:assert/strict');
const {page, flush} = require('./helpers.cjs');
test('snapshot omits password/file controls and never includes input value', async () => {
  const h = page(), grant = await h.grant(), snapshot = (await grant.send('snapshot')).result;
  assert.equal(snapshot.elements.length, 2);
  assert.equal(JSON.stringify(snapshot).includes('private field content'), false);
});
test('unrelated DOM mutation does not invalidate an unchanged target', async () => {
  const h = page(), grant = await h.grant(), s = (await grant.send('snapshot')).result; h.changed();
  assert.equal((await grant.send('click', {snapshotId: s.snapshotId, elementId: 1})).result.clicked, true);
  assert.equal(h.button.clicks, 1);
});
for (const change of ['label', 'covered', 'disabled', 'busy', 'removed', 'geometry', 'expired', 'scroll']) {
  test(`${change} rejects action before its side effect`, async () => {
    const h = page(), grant = await h.grant(), s = (await grant.send('snapshot')).result;
    if (change === 'label') h.button.textContent = 'Delete instead';
    if (change === 'covered') h.cover();
    if (change === 'disabled') h.button.attrs['aria-disabled'] = 'true';
    if (change === 'busy') h.button.attrs['aria-busy'] = 'true';
    if (change === 'removed') h.button.isConnected = false;
    if (change === 'geometry') h.button.rect = {...h.button.rect, top: 30, bottom: 50};
    if (change === 'expired') h.advance(30001);
    if (change === 'scroll') h.fire('scroll');
    assert.ok((await grant.send('click', {snapshotId: s.snapshotId, elementId: 1})).error);
    assert.equal(h.button.clicks, 0);
  });
}
test('a field edited since observation is not overwritten', async () => {
  const h = page(), grant = await h.grant(), s = (await grant.send('snapshot')).result;
  h.input.value = 'user edit';
  assert.match((await grant.send('type', {snapshotId: s.snapshotId, elementId: 2, text: 'overwrite'})).error, /Stale snapshot/);
  assert.equal(h.input.value, 'user edit');
});
test('select is observable but does not claim to open a native picker', async () => {
  const h = page(); h.button.tagName = 'SELECT';
  const grant = await h.grant(), snapshot = (await grant.send('snapshot')).result;
  assert.equal(snapshot.elements[0].actions.length, 0);
  assert.match((await grant.send('click', {snapshotId: snapshot.snapshotId, elementId: 1})).error, /not supported/);
  assert.equal(h.button.clicks, 0);
});
test('click consumes snapshot and cannot be replayed', async () => {
  const h = page(), grant = await h.grant(), s = (await grant.send('snapshot')).result;
  assert.equal((await grant.send('click', {snapshotId: s.snapshotId, elementId: 1})).result.clicked, true);
  assert.match((await grant.send('click', {snapshotId: s.snapshotId, elementId: 1})).error, /Stale snapshot/);
  assert.equal(h.button.clicks, 1);
});
test('typing replaces text, dispatches events and consumes snapshot', async () => {
  const h = page(), grant = await h.grant(), s = (await grant.send('snapshot')).result;
  assert.equal((await grant.send('type', {snapshotId: s.snapshotId, elementId: 2, text: 'hello'})).result.typed, true);
  assert.equal(h.input.value, 'hello'); assert.deepEqual(h.input.events, ['input', 'change']);
});
test('reinjecting and regranting the same document invalidates the old snapshot', async () => {
  const h = page(), first = await h.grant(), s = (await first.send('snapshot')).result;
  h.inject(); const next = await h.grant();
  assert.equal(next.context.documentId, first.context.documentId);
  assert.notEqual(next.context.grantId, first.context.grantId);
  assert.match((await next.send('click', {snapshotId: s.snapshotId, elementId: 1})).error, /Stale snapshot/);
  assert.equal(h.button.clicks, 0);
});
test('old context on new channel is rejected before execution', async () => {
  const h = page(), first = await h.grant(), s = (await first.send('snapshot')).result, next = await h.grant();
  next.pair.background.postMessage({kind: 'command', id: '1', method: 'click', params: {snapshotId: s.snapshotId, elementId: 1}, context: first.context});
  await flush(); assert.equal(h.button.clicks, 0); assert.equal(next.pair.content.closed, true);
});
test('delayed old-channel commands cannot execute against a replacement grant', async () => {
  const h = page(), first = await h.grant(), s = (await first.send('snapshot')).result;
  await h.grant();
  first.pair.content.onMessage.emit({kind: 'command', id: '1', method: 'click', params: {snapshotId: s.snapshotId, elementId: 1}, context: first.context});
  assert.equal(h.button.clicks, 0);
});
