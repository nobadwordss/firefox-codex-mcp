const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const flush = () => new Promise(resolve => setImmediate(resolve));
async function popup(state, failure) {
  const elements = new Map();
  const element = id => {
    if (!elements.has(id)) elements.set(id, {hidden:false,disabled:false,textContent:''});
    return elements.get(id);
  };
  const requests = [];
  let poll;
  vm.runInNewContext(fs.readFileSync(require.resolve('../native/popup.js'),'utf8'), {
    document:{querySelector:element},
    browser:{runtime:{async sendMessage({action}) {
      requests.push(action);
      if (action === 'settings' && failure) throw Error(failure);
      return state;
    }}},
    setInterval(fn) {poll=fn;return 1;}
  });
  await flush();
  return {element,requests,poll:()=>poll()};
}
const automatic={connected:true,enabled:true,status:'Connected',selected:false,unselected:true,profileCount:1,siteAccess:true,chats:0,tabs:0};
test('one Firefox connects without presenting a profile choice', async()=>{
  const ui=await popup(automatic);
  assert.equal(ui.element('#choose').hidden,true);
  assert.match(ui.element('#profile').textContent,/automatically/);
});
test('choice is visible only when an unselected profile can be chosen',async()=>{
  for(const state of [{...automatic,profileCount:2},{...automatic,unselected:false}]){
    const ui=await popup(state);assert.equal(ui.element('#choose').hidden,false);
  }
  for(const state of [{...automatic,selected:true},{...automatic,connected:false}]){
    const ui=await popup(state);assert.equal(ui.element('#choose').hidden,true);
  }
});
test('Settings upgrade errors survive polling and the button becomes usable again',async()=>{
  const ui=await popup(automatic,'Update the local helper from a newer download');
  await ui.element('#settings').onclick();
  assert.equal(ui.element('#settings').disabled,false);
  assert.equal(ui.element('#error').hidden,false);
  assert.match(ui.element('#error').textContent,/Update the local helper/);
  await ui.poll();
  assert.match(ui.element('#error').textContent,/Update the local helper/);
  assert.equal(ui.requests.filter(action=>action==='settings').length,1);
});
