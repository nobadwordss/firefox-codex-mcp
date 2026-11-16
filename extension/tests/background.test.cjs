const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const protocol = require('../protocol.js');
function setup() {
  const events = {}, sockets = [], stored = {}, sent = [];
  let tab = {id:10,url:'https://example.org/',incognito:false,status:'complete'};
  const event = name => ({addListener(fn) {events[name]=fn;}});
  class Socket {
    static OPEN=1;
    constructor(url) { this.url=url; this.readyState=1; this.messages=[]; sockets.push(this); }
    send(value) {this.messages.push(JSON.parse(value));}
    close() {this.closed=true;this.onclose?.();}
  }
  const browser = {
    runtime:{id:'extension',getURL:path=>`moz-extension://test/${path}`,onMessage:event('message')},
    storage:{local:{async get() {return stored;},async set(value) {Object.assign(stored,value);}}},
    tabs:{onRemoved:event('removed'),onUpdated:event('updated'),async query() {return [tab];},async get() {return tab;},async executeScript() {},async sendMessage(id,request) {sent.push(request);return {result:{snapshotId:'s',elements:[]}};}}
  };
  vm.runInNewContext(fs.readFileSync(require.resolve('../background.js'),'utf8'),{browser,FirefoxCodexProtocol:protocol,WebSocket:Socket,TextEncoder,setTimeout:()=>1,clearTimeout:()=>{}});
  const popup={id:'extension',url:'moz-extension://test/popup.html'};
  return {events,sockets,sent,stored,send:message=>events.message(message,popup),setTab:value=>{tab=value;}};
}
async function connected(h) {
  await h.send({kind:'connect',token:'a'.repeat(64)});
  const socket=h.sockets[0];socket.onopen();await socket.onmessage({data:JSON.stringify({type:'ready'})});return socket;
}
test('auth precedes requests; no page grant by default', async()=>{
  const h=setup(),s=await connected(h);
  assert.equal(s.url,'ws://127.0.0.1:8765');assert.deepEqual(s.messages[0],{type:'auth',token:'a'.repeat(64)});
  await s.onmessage({data:JSON.stringify({id:'1',method:'snapshot',params:{}})});
  assert.match(s.messages[1].error,/No tab enabled/);assert.equal(h.sent.length,0);
});
test('only own popup may grant or configure',async()=>{
  const h=setup();
  await h.events.message({kind:'connect',token:'a'.repeat(64)},{id:'extension',url:'https://example.org/'});
  assert.equal(h.sockets.length,0);
});
test('explicit grant routes commands and navigation revokes it',async()=>{
  const h=setup(),s=await connected(h);
  await h.send({kind:'enable'});
  await s.onmessage({data:JSON.stringify({id:'1',method:'snapshot',params:{}})});
  assert.equal(h.sent.length,1);assert.equal(s.messages[1].result.snapshotId,'s');
  h.events.updated(10,{status:'loading'});
  assert.equal((await h.send({kind:'status'})).tabId,null);
});
test('disconnect revokes access and closes transport',async()=>{
  const h=setup(),s=await connected(h);await h.send({kind:'enable'});await h.send({kind:'disconnect'});
  assert.equal((await h.send({kind:'status'})).tabId,null);assert.equal(s.closed,true);
});
test('connection loss revokes access; untrusted message cannot authenticate',async()=>{
  const h=setup(),s=await connected(h);await h.send({kind:'enable'});s.close();
  assert.equal((await h.send({kind:'status'})).tabId,null);
  const other=setup();await other.send({kind:'connect',token:'a'.repeat(64)});const bad=other.sockets[0];bad.onopen();await bad.onmessage({data:'{"type":"unexpected"}'});assert.equal(bad.closed,true);
});
