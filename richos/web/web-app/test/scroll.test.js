'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const {attach}=require('../lib/follow.js');
function setup() {
  const handlers={},observed=[];
  const area={scrollTop:0,scrollHeight:1600,clientHeight:600,clientWidth:360,addEventListener:(name,fn)=>handlers[name]=fn,removeEventListener:name=>delete handlers[name]};
  const content={},latest={hidden:true};let resize;
  class Observer {constructor(fn){resize=fn;}observe(node){observed.push(node);}disconnect(){observed.length=0;}}
  const follow=attach(area,content,latest,{ResizeObserver:Observer});
  follow.resume();
  return {area,content,latest,follow,observed,resize:()=>resize(),event:(name,event={})=>handlers[name](event)};
}
test('first render, streamed growth and viewport changes stay at the newest message',()=>{
  const p=setup();assert.equal(p.area.scrollTop,1000);
  assert.deepEqual(p.observed,[p.area,p.content]);
  p.area.scrollHeight+=130;p.resize();assert.equal(p.area.scrollTop,1130);
  p.area.clientHeight-=93;p.event('scroll');p.resize();assert.equal(p.area.scrollTop,1223);
  p.area.clientHeight+=93;p.area.scrollTop=1130;p.event('scroll');p.resize();
  assert.equal(p.follow.following,true);assert.equal(p.latest.hidden,true);
});
for(const input of ['wheel','touch','keyboard'])test(`even a small deliberate upward ${input} movement pauses following`,()=>{
  const p=setup();
  if(input==='wheel')p.event('wheel',{deltaY:-8});
  if(input==='touch'){p.event('touchstart',{touches:[{clientY:200}]});p.event('touchmove',{touches:[{clientY:208}]});}
  if(input==='keyboard')p.event('keydown',{key:'ArrowUp'});
  p.area.scrollTop-=8;p.event('scroll');
  assert.equal(p.follow.following,false);assert.equal(p.latest.hidden,false);
  const before=p.follow.capture();p.area.scrollHeight+=220;p.follow.restore(before);p.resize();
  assert.equal(p.area.scrollTop,992,'incoming content must not steal the reading position');
  p.latest.onclick();assert.equal(p.area.scrollTop,1220);assert.equal(p.follow.following,true);
});
test('sending resumes following and prepending older history preserves the reading position',()=>{
  const p=setup();p.event('wheel',{deltaY:-500});p.area.scrollTop=100;p.event('scroll');
  const before=p.follow.capture();p.area.scrollHeight+=500;p.follow.restore(before,{prepend:true});
  assert.equal(p.area.scrollTop,600);assert.equal(p.follow.following,false);
  p.follow.restore(p.follow.capture(),{force:true});assert.equal(p.area.scrollTop,1500);assert.equal(p.follow.following,true);
});
test('notification focus holds its place until Latest or an explicit send',()=>{
  const p=setup();p.follow.focus({scrollIntoView(){p.area.scrollTop=300;}});
  p.area.scrollHeight+=100;p.resize();assert.equal(p.area.scrollTop,300);assert.equal(p.follow.following,false);
  p.follow.resume();assert.equal(p.area.scrollTop,1100);p.follow.close();assert.equal(p.observed.length,0);
});
// KNOWN DEFECT follow-yank (reported, not fixed: CEO §76 preserves the PWA and the iPhone client).
// WebKit and Chromium both move the view BEFORE they dispatch the wheel, so a fling to the oldest
// message reaches pause() with scrollTop 0 and pause() does nothing; a layout change in the same
// frame then sends the scroll handler down its "geometry changed while following" branch, which
// pins him back to the newest message. Measured in real engines by a browser test since removed. This
// fails the day the defect is fixed, so the expectation cannot outlive it.
test('KNOWN DEFECT follow-yank — a fling to the top with a layout change in the same frame is returned to the newest message',()=>{
  const p=setup();
  p.area.scrollTop=0;p.area.clientHeight-=30;
  p.event('wheel',{deltaY:-50000});p.event('scroll');p.resize();
  assert.notEqual(p.area.scrollTop,0,'follow-yank is no longer reproduced: replace this expectation with the ordinary assertion');
  assert.equal(p.follow.following,true);assert.equal(p.latest.hidden,true);
});
