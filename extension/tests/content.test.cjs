const test=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
function page() {
  let receive,changed,seq=0;
  class Element {
    constructor(tag='BUTTON'){this.tagName=tag;this.textContent='Press';this.isConnected=true;this.attrs={};this.events=[];}
    getBoundingClientRect(){return {width:40,height:20,bottom:20,right:40,top:0,left:0};}
    getAttribute(name){return this.attrs[name]??null;}
    closest(){return null;}
    click(){this.clicked=true;}
    dispatchEvent(event){this.events.push(event.type);}
  }
  class Input extends Element {
    constructor(type='text'){super('INPUT');this.type=type;this.autocomplete='';this._value='private field content';}
    set value(value){this._value=value;}
    get value(){return this._value;}
  }
  class Textarea extends Element {
    constructor(){super('TEXTAREA');}
    set value(value){this._value=value;}
  }
  const button=new Element(),input=new Input(),password=new Input('password'),file=new Input('file');
  const elements=[button,input,password,file];
  class Observer {constructor(callback){changed=callback;}observe(){}takeRecords(){return [];}}
  const context={document:{title:'Example',querySelectorAll:()=>elements},location:{href:'https://example.org/'},innerWidth:800,innerHeight:600,MutationObserver:Observer,addEventListener(){},getComputedStyle:()=>({visibility:'visible',display:'block',opacity:'1'}),HTMLInputElement:Input,HTMLTextAreaElement:Textarea,Event:class{constructor(type){this.type=type;}},crypto:{randomUUID:()=>`s${++seq}`},URL,browser:{runtime:{id:'extension',onMessage:{addListener(fn){receive=fn;}}}},window:{scrollBy(){}}};
  vm.runInNewContext(fs.readFileSync(require.resolve('../content.js'),'utf8'),context);
  return {button,input,changed:()=>changed(),send:(method,params={})=>receive({kind:'command',method,params},{id:'extension'})};
}
test('snapshot omits password/file controls and never includes input value',async()=>{
  const h=page(),s=(await h.send('snapshot')).result;
  assert.equal(s.elements.length,2);assert.equal(JSON.stringify(s).includes('private field content'),false);
});
test('DOM mutation makes element IDs stale',async()=>{
  const h=page(),s=(await h.send('snapshot')).result;h.changed();
  assert.match((await h.send('click',{snapshotId:s.snapshotId,elementId:1})).error,/Stale snapshot/);assert.equal(h.button.clicked,undefined);
});
test('click consumes snapshot and cannot be replayed',async()=>{
  const h=page(),s=(await h.send('snapshot')).result;
  assert.equal((await h.send('click',{snapshotId:s.snapshotId,elementId:1})).result.clicked,true);
  assert.match((await h.send('click',{snapshotId:s.snapshotId,elementId:1})).error,/Stale snapshot/);
});
test('typing replaces plain text, dispatches events and consumes snapshot',async()=>{
  const h=page(),s=(await h.send('snapshot')).result;
  assert.equal((await h.send('type',{snapshotId:s.snapshotId,elementId:2,text:'hello'})).result.typed,true);
  assert.equal(h.input.value,'hello');assert.deepEqual(h.input.events,['input','change']);
});
