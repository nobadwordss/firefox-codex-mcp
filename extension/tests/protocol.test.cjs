const test = require('node:test');
const assert = require('node:assert/strict');
const p = require('../protocol.js');
const request = (method, params) => ({id:'1',method,params});
test('tokens must be 32 bytes of hex', () => {
  assert.equal(p.validToken('a'.repeat(64)), true);
  for (const token of ['', 'a'.repeat(63), 'z'.repeat(64), null, 17]) assert.equal(p.validToken(token), false);
});
test('pairing codes accept only a bounded loopback port and full instance ID', () => {
  assert.equal(p.connectionIdentity(`45001:${'a'.repeat(32)}`).port,45001);
  for (const code of ['ws://evil.test:45001',`80:${'a'.repeat(32)}`,`65536:${'a'.repeat(32)}`,`45001:${'a'.repeat(31)}`,`45001:${'a'.repeat(32)}/path`]) assert.throws(()=>p.connectionIdentity(code));
});
test('mutual authentication proofs bind role, client and fresh challenge', async () => {
  const token='a'.repeat(64), id='b'.repeat(32), nonce='c'.repeat(64);
  const proof=await p.authProof(token,id,nonce,'extension');
  assert.equal(await p.verifyProof(token,id,nonce,'extension',proof),true);
  assert.equal(await p.verifyProof(token,id,nonce,'server',proof),false);
  assert.equal(await p.verifyProof(token,'d'.repeat(32),nonce,'extension',proof),false);
  assert.equal(await p.verifyProof(token,id,'e'.repeat(64),'extension',proof),false);
});
test('page scheme allowlist excludes privileged and executable URLs', () => {
  assert.equal(p.safePage('https://example.org'), true);
  assert.equal(p.safePage('http://localhost'), true);
  for (const url of ['about:config','file:///tmp/test','javascript:alert(1)','moz-extension://x/', 'broken']) assert.equal(p.safePage(url), false);
});
test('snapshot and bounded actions accepted', () => {
  for (const r of [request('snapshot',{}),request('click',{snapshotId:'s',elementId:1}),request('type',{snapshotId:'s',elementId:2,text:'hello'}),request('scroll',{snapshotId:'s',y:100})]) assert.equal(p.validateRequest(r),r);
});
test('missing or unsafe argument shapes rejected', () => {
  for (const r of [null,[],request('eval',{}),request('snapshot',null),request('click',{elementId:1}),request('click',{snapshotId:'s',elementId:0}),request('type',{snapshotId:'s',elementId:1,text:'a'.repeat(10001)}),request('scroll',{snapshotId:'s',y:Infinity}),request('scroll',{snapshotId:'s',y:2001}),{id:4,method:'snapshot',params:{}}]) assert.throws(()=>p.validateRequest(r));
});
test('manifest has explicit user grants and no global host access', () => {
  const manifest = require('../manifest.json');
  assert.equal(manifest.manifest_version,2);
  assert.deepEqual(manifest.permissions,['activeTab','tabs','storage','http://127.0.0.1/*']);
  assert.equal(manifest.incognito,'not_allowed');
  assert.equal(manifest.content_scripts,undefined);
});
