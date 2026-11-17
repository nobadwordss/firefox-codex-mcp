const test=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.join(__dirname,'..'),flush=()=>new Promise(r=>setImmediate(r));
function event(){const listeners=[];return {addListener:f=>listeners.push(f),emit:(...args)=>listeners.map(f=>f(...args))};}
async function setup(capabilities=[]){
  const messages=[],ports=[],tabs=new Map(),control=event(),updated=event(),removed=event(),alarms=event(),timers=new Map();let next=10,timerId=0;
  const browser={runtime:{id:'extension',getURL:p=>'moz-extension://test/'+p,onMessage:control,connectNative(){const port={onMessage:event(),onDisconnect:event(),postMessage:m=>messages.push(m),disconnect(){this.onDisconnect.emit();}};ports.push(port);return port;}},storage:{local:{async get(){return {};},async set(){}}},tabs:{onUpdated:updated,onRemoved:removed,async create(opts){const tab={id:next++,url:opts.url,status:'complete'};tabs.set(tab.id,tab);return tab;},async get(id){if(!tabs.has(id))throw Error('gone');return tabs.get(id);},async remove(id){tabs.delete(id);removed.emit(id);},async update(id,opts){Object.assign(tabs.get(id),opts);updated.emit(id,{url:opts.url});}},permissions:{onRemoved:event(),async contains(){return true;}},alarms:{onAlarm:alarms,create(){}}};
  const context=vm.createContext({browser,crypto:crypto.webcrypto,URL,Map,Promise,Date,setTimeout:callback=>{const id=++timerId;timers.set(id,callback);return id;},clearTimeout:id=>timers.delete(id)});
  for(const file of ['protocol.js','native/background.js'])vm.runInContext(fs.readFileSync(path.join(root,file),'utf8'),context);
  await flush();const port=ports[0];port.onMessage.emit({kind:'ready',capabilities});port.onMessage.emit({kind:'configuration',selected:null,count:1});let seq=0;
  const bindings=new Map();
  function attach(owner='a'.repeat(32),attachment='1'.repeat(32)){bindings.set(owner,attachment);port.onMessage.emit({kind:'attachOwner',owner,attachment});}
  function suspend(owner='a'.repeat(32),attachment=bindings.get(owner)){port.onMessage.emit({kind:'suspendOwner',owner,attachment});}
  function raw(id,name,args={},owner='a'.repeat(32),attachment=bindings.get(owner)){port.onMessage.emit({kind:'request',id,owner,attachment,name,args});}
  async function call(name,args={},owner='a'.repeat(32)){if(!bindings.has(owner))attach(owner);const id=String(++seq);raw(id,name,args,owner);await flush();return messages.find(m=>m.id===id&&m.kind==='response');}
  const send=action=>control.emit({action},{id:'extension',url:'moz-extension://test/native/popup.html'})[0];
  attach();
  return {browser,messages,ports,port,tabs,updated,alarms,call,send,control,attach,suspend,raw,timers};
}
test('native work tabs are created without grants; existing and other chat tabs stay unavailable',async()=>{const h=await setup();h.tabs.set(1,{id:1,url:'https://private.test'});assert.equal((await h.call('firefox_tabs')).result.tabs.length,0);const opened=await h.call('firefox_open_tab',{url:'https://example.com'});assert.equal(opened.result.tabId,'10');assert.match((await h.call('firefox_select_tab',{tabId:'10'},'b'.repeat(32))).error,/not owned/);assert.match((await h.call('firefox_select_tab',{tabId:'1'})).error,/not owned/);});
test('revoke fences pending creation and queued work; new request may create a fresh tab',async()=>{const h=await setup();let finish;h.browser.tabs.create=()=>new Promise(r=>finish=r);h.raw('pending','firefox_open_tab',{url:'https://example.com'});await flush();h.raw('queued','firefox_tabs');h.port.onMessage.emit({kind:'dropOwner',owner:'a'.repeat(32)});finish({id:42});await flush();assert.match(h.messages.find(m=>m.id==='pending').error,/released/);assert.match(h.messages.find(m=>m.id==='queued').error,/released/);h.attach();assert.equal((await h.call('firefox_status')).result.ownedTabs,0);});
test('pause persists, status remains readable, resume does not reacquire released tabs',async()=>{const h=await setup();await h.call('firefox_open_tab',{url:'https://example.com'});await h.send('pause');assert.equal((await h.call('firefox_status')).result.enabled,false);assert.match((await h.call('firefox_open_tab',{url:'https://example.com'})).error,/released/);await h.send('resume');assert.equal((await h.call('firefox_status')).result.ownedTabs,0);});
test('connection loss releases all tabs; alarm reconnects without a popup',async()=>{const h=await setup();await h.call('firefox_open_tab',{url:'https://example.com'});h.port.onDisconnect.emit();h.alarms.emit();assert.equal(h.ports.length,2);h.ports[1].onMessage.emit({kind:'ready'});assert.equal((await h.send('status')).tabs,0);});
test('page messages cannot select profile or resume the extension',async()=>{const h=await setup();await assert.rejects(h.control.emit({action:'choose'},{id:'extension',url:'https://example.com'})[0],/Only extension/);assert.equal(h.messages.some(m=>m.kind==='selectProfile'),false);});
test('default manifest has native transport with no network bridge or static page injection',()=>{const m=JSON.parse(fs.readFileSync(path.join(root,'manifest.json')));assert.equal(m.manifest_version,3);assert.ok(m.permissions.includes('nativeMessaging'));assert.equal(m.content_scripts,undefined);assert.deepEqual(m.host_permissions,['http://*/*','https://*/*']);assert.equal(m.incognito,'not_allowed');});

test('reattachment preserves the exact tab, fences old requests and late EOF',async()=>{
  const h=await setup(),owner='a'.repeat(32),old='1'.repeat(32),next='2'.repeat(32);
  const tab=(await h.call('firefox_open_tab',{url:'https://example.com'})).result.tabId;
  h.suspend();
  assert.match((await h.call('firefox_tabs')).error,/attachment/);
  h.attach(owner,next);
  h.suspend(owner,old);
  assert.equal((await h.call('firefox_tabs')).result.tabs[0].tabId,tab);
  h.raw('stale','firefox_close_tab',{},owner,old);await flush();
  assert.match(h.messages.find(m=>m.id==='stale').error,/attachment/);
  assert.equal(h.tabs.size,1);
  assert.match((await h.call('firefox_click',{snapshotId:'old',elementId:1})).error,/fresh snapshot/);
});
test('toolbar revocation while detached never resurrects tabs on resume',async()=>{
  const h=await setup(),owner='a'.repeat(32);
  const tab=(await h.call('firefox_open_tab',{url:'https://example.com'})).result.tabId;
  h.suspend();await h.send('revoke');h.attach(owner,'2'.repeat(32));
  assert.equal((await h.call('firefox_status')).result.ownedTabs,0);
  assert.match((await h.call('firefox_select_tab',{tabId:tab})).error,/not owned/);
  assert.equal(h.tabs.size,1);
});
test('replacement fences pending creation and its queued commands without clearing retained tabs',async()=>{
  const h=await setup(),owner='a'.repeat(32);
  await h.call('firefox_open_tab',{url:'https://example.com'});
  let finish;h.browser.tabs.create=()=>new Promise(r=>finish=r);
  h.raw('pending','firefox_open_tab',{url:'https://example.org'});await flush();
  h.raw('queued','firefox_close_tab');h.attach(owner,'2'.repeat(32));finish({id:42});await flush();
  assert.match(h.messages.find(m=>m.id==='pending').error,/released/);
  assert.match(h.messages.find(m=>m.id==='queued').error,/released/);
  assert.equal((await h.call('firefox_tabs')).result.tabs[0].tabId,'10');
});
test('chat revoke stays isolated and is irreversible across attachments',async()=>{
  const h=await setup(),b='b'.repeat(32);
  const aTab=(await h.call('firefox_open_tab',{url:'https://example.com'})).result.tabId;
  const bTab=(await h.call('firefox_open_tab',{url:'https://example.org'},b)).result.tabId;
  await h.call('firefox_revoke');h.attach('a'.repeat(32),'2'.repeat(32));
  assert.match((await h.call('firefox_select_tab',{tabId:aTab})).error,/not owned/);
  assert.match((await h.call('firefox_select_tab',{tabId:bTab})).error,/not owned/);
  assert.equal((await h.call('firefox_tabs',{},b)).result.tabs[0].tabId,bTab);
});
test('EOF during tab creation cannot add the delayed tab to retained ownership',async()=>{
  const h=await setup(),owner='a'.repeat(32);
  const retained=(await h.call('firefox_open_tab',{url:'https://example.com'})).result.tabId;
  let finish;h.browser.tabs.create=()=>new Promise(r=>finish=r);
  h.raw('pending','firefox_open_tab',{url:'https://example.org'});await flush();
  h.raw('queued','firefox_close_tab');h.suspend();
  h.tabs.set(42,{id:42,url:'https://example.org',status:'complete'});finish(h.tabs.get(42));await flush();
  assert.match(h.messages.find(m=>m.id==='pending').error,/released/);
  assert.match(h.messages.find(m=>m.id==='queued').error,/released/);
  h.attach(owner,'2'.repeat(32));
  assert.equal((await h.call('firefox_tabs')).result.tabs[0].tabId,retained);
  assert.equal((await h.call('firefox_tabs')).result.tabs.length,1);
  assert.match((await h.call('firefox_select_tab',{tabId:'42'})).error,/not owned/);
});

test('Settings requires the trusted popup and a capable helper',async()=>{
  const h=await setup();
  await assert.rejects(h.send('settings'),/Update the local helper/);
  await assert.rejects(h.control.emit({action:'settings'},{id:'extension',url:'https://example.com'})[0],/Only extension/);
  assert.equal(h.messages.some(m=>m.kind==='openSettings'),false);
  assert.equal((await h.send('status')).connected,true);
});
test('Settings opens in this Firefox without giving any chat ownership',async()=>{
  const h=await setup(['settings-ui-v1']);
  const pending=h.send('settings');await flush();
  const request=h.messages.find(m=>m.kind==='openSettings');
  assert.ok(request);
  const url=`http://127.0.0.1:45678/#${'a'.repeat(64)}`;
  h.port.onMessage.emit({kind:'settingsResult',id:'unsolicited',url});
  assert.equal(h.tabs.size,0);
  await assert.rejects(h.send('settings'),/already opening/);
  h.port.onMessage.emit({kind:'settingsResult',id:request.id,url});
  await pending;
  assert.equal([...h.tabs.values()][0].url,url);
  assert.equal((await h.call('firefox_tabs')).result.tabs.length,0);
  assert.match((await h.call('firefox_select_tab',{tabId:'10'})).error,/not owned/);
});
test('Settings rejects unsafe addresses and stale native replies',async()=>{
  for(const url of ['https://127.0.0.1:45678/#','http://localhost:45678/#','http://127.0.0.1:65536/#','http://127.0.0.1:0/#','http://127.0.0.1:45678/path#','http://127.0.0.1:45678/?query#','http://user@127.0.0.1:45678/#']){
    const h=await setup(['settings-ui-v1']),pending=h.send('settings');await flush();
    const request=h.messages.find(m=>m.kind==='openSettings');
    h.port.onMessage.emit({kind:'settingsResult',id:request.id,url:url+'a'.repeat(64)});
    await assert.rejects(pending,/Invalid Settings address/);assert.equal(h.tabs.size,0);
  }
  const h=await setup(['settings-ui-v1']),pending=h.send('settings');await flush();
  const request=h.messages.find(m=>m.kind==='openSettings');
  const rejection=assert.rejects(pending,/Connection ended/);
  h.port.onDisconnect.emit();await rejection;
  h.port.onMessage.emit({kind:'settingsResult',id:request.id,url:`http://127.0.0.1:45678/#${'a'.repeat(64)}`});
  assert.equal(h.tabs.size,0);
});
test('MCP cannot request Settings as a browser tool',async()=>{
  const h=await setup(['settings-ui-v1']);
  assert.match((await h.call('openSettings')).error,/Unsupported command|No AI work tab/);
  assert.equal(h.messages.some(m=>m.kind==='openSettings'),false);
});

test('Settings timeout rejects late replies and permits a fresh attempt',async()=>{
  const h=await setup(['settings-ui-v1']),pending=h.send('settings');await flush();
  const request=h.messages.find(m=>m.kind==='openSettings');
  const rejection=assert.rejects(pending,/did not open|timed out|did not respond/);
  [...h.timers.values()][0]();await rejection;
  h.port.onMessage.emit({kind:'settingsResult',id:request.id,url:`http://127.0.0.1:45678/#${'a'.repeat(64)}`});
  assert.equal(h.tabs.size,0);
  const fresh=h.send('settings');await flush();
  const next=h.messages.filter(m=>m.kind==='openSettings').at(-1);
  assert.notEqual(next.id,request.id);
  h.port.onMessage.emit({kind:'settingsResult',id:next.id,error:'Could not start connection settings'});
  await assert.rejects(fresh,/Could not start/);assert.equal(h.tabs.size,0);
});
