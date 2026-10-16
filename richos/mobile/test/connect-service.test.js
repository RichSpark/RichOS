const test=require('node:test'),assert=require('node:assert/strict');
const {fixture}=require('./connect-fixture.cjs');
const {join}=require('node:path');
test('signed host enrolls once, retrieves only its token and cannot impersonate another host', async t => {
  const f=await fixture(t), a=await f.identity(), b=await f.identity();
  let response=await f.send(await a.request('POST','/v1/hosts'));
  assert.equal(response.status,200); const first=await response.json(); assert.equal(first.phase,'active');
  assert(!JSON.stringify(first).includes('token'));
  response=await f.send(await a.request('POST','/v1/hosts')); assert.equal(response.status,200);
  assert.equal(f.tunnels.size,1); assert.equal(f.dns.size,1);
  assert.equal((await f.send(await b.request('POST','/v1/host/token'))).status,404);
  const token=await (await f.send(await a.request('POST','/v1/host/token'))).json();
  assert.equal(token.token,'scoped-token-for-'+a.id); assert(!JSON.stringify(token).includes(f.env.CF_API_TOKEN));
  assert.equal((await f.send(await b.request('POST','/v1/hosts'))).status,200);
  const second=await (await f.send(await b.request('GET','/v1/host'))).json(); assert.notEqual(second.endpoint,first.endpoint);
});

test('service signatures bind method, path, body, timestamp and a single-use nonce', async t => {
  const f=await fixture(t), a=await f.identity();
  const enroll=await a.request('POST','/v1/hosts');
  assert.equal((await f.send(enroll.clone())).status,200);
  assert.equal((await f.send(enroll)).status,409);
  for(const mode of ['method','path','body','key','signature']) {
    const good=await a.request('POST','/v1/host/token');
    const headers=new Headers(good.headers);
    if(mode==='key') headers.set('x-richos-key',(await f.identity()).publicKey);
    if(mode==='signature') headers.set('x-richos-signature','a'.repeat(86));
    const bad=new Request('https://connect.example.com'+(mode==='path'?'/v1/hosts':'/v1/host/token'),{method:mode==='method'?'DELETE':'POST',headers,body:mode==='body'?'{}':''});
    assert.equal((await f.send(bad)).status,404,mode);
  }
  assert.equal((await f.send(await a.request('GET','/v1/host','',{time:1790036000000}))).status,404);
});

test('no admission step: a new Mac enrolls, disables and re-enables on its own signed requests (CEO 2026-10-06)', async t => {
  // "Nothing is in a private pilot." The uploaded bindings carry no admission switch and the
  // schema no admission table, so this runs on exactly what is deployed.
  const f=await fixture(t), a=await f.identity();
  assert.equal(f.env.ENROLLMENT_OPEN,undefined,'the deployed bindings have no admission switch');
  let reply=await f.send(await a.request('POST','/v1/hosts'));
  assert.equal(reply.status,200); assert.equal((await reply.json()).phase,'active');
  assert.equal((await f.send(await a.request('DELETE','/v1/host'))).status,200);
  reply=await f.send(await a.request('POST','/v1/hosts'));
  assert.equal(reply.status,200); const view=await reply.json();
  assert.equal(view.phase,'active'); assert.equal(view.generation,2); assert.equal(f.tunnels.size,1);
  // The lifecycle itself takes no admission option, and enable succeeds without one.
  const { transition }=await import('../service/connect/lifecycle.mjs');
  assert.equal((await f.send(await a.request('DELETE','/v1/host'))).status,200);
  assert.equal((await transition(f.store,f.provider,a.id,'enable','example.com')).phase,'active');
  assert.equal(await f.store.statement("SELECT name FROM sqlite_master WHERE name='allowed_hosts'").first(),null,
    'the schema still creates an admission table');
});

test('capacity stays bounded, and a full service answers capacity_reached, never an admission', async t => {
  const f=await fixture(t), a=await f.identity(), b=await f.identity();
  f.env.HOST_CAPACITY='1000';
  assert.equal((await f.send(await a.request('GET','/v1/host'))).status,404,'a Worker configured for 1000 hosts serves requests');
  f.env.HOST_CAPACITY='1';
  assert.equal((await f.send(await a.request('POST','/v1/hosts'))).status,200);
  const full=await f.send(await b.request('POST','/v1/hosts'));
  assert.equal(full.status,503); assert.deepEqual(await full.json(),{error:'capacity_reached'});
  assert.equal(f.tunnels.size,1); assert.equal(await f.store.get(b.id),null);
});

for(const stage of ['create','configure','dns']) test(`provisioning recovers after lost ${stage} response without duplicate resources`, async t => {
  const f=await fixture(t), a=await f.identity(); f.fail(stage);
  assert.equal((await f.send(await a.request('POST','/v1/hosts'))).status,503);
  assert.equal((await f.store.get(a.id)).phase,'pending');
  await f.reconcile();
  assert.equal((await f.store.get(a.id)).phase,'active');
  assert.equal(f.tunnels.size,1); assert.equal(f.dns.size,1);
});

test('disable survives provider failure, denies tokens immediately and reconciles before reenabling', async t => {
  const f=await fixture(t), a=await f.identity();
  const old=await (await f.send(await a.request('POST','/v1/hosts'))).json();
  f.fail('remove'); assert.equal((await f.send(await a.request('DELETE','/v1/host'))).status,503);
  assert.equal((await f.send(await a.request('POST','/v1/host/token'))).status,410);
  await f.reconcile(); assert.equal(f.tunnels.size,0); assert.equal(f.dns.size,0);
  const next=await (await f.send(await a.request('POST','/v1/hosts'))).json();
  assert.equal(next.generation,2); assert.notEqual(next.endpoint,old.endpoint);
});

test('an expired lease cannot write or release its replacement', async t => {
  const f=await fixture(t), a=await f.identity();
  await f.send(await a.request('POST','/v1/hosts'));
  const old=await f.store.lease(a.id); assert(old);
  assert.equal(await f.store.lease(a.id),null);
  f.advance(120001); const next=await f.store.lease(a.id); assert(next);
  await assert.rejects(f.store.write(a.id,old,{desired:0}),/lease_lost/);
  await f.store.release(a.id,old); assert.equal((await f.store.get(a.id)).lease_id,next);
});

test('device registration is host-owned and generation-bound; disable clears it', async t => {
  const f=await fixture(t), a=await f.identity();
  await f.send(await a.request('POST','/v1/hosts'));
  assert.equal((await f.send(await a.request('PUT','/v1/host/device',JSON.stringify({generation:2,deviceKeyHash:'c'.repeat(64)})))).status,400);
  assert.equal((await f.send(await a.request('PUT','/v1/host/device',JSON.stringify({generation:1,deviceKeyHash:'c'.repeat(64)})))).status,200);
  assert.equal((await f.store.get(a.id)).device_key_hash,'c'.repeat(64));
  await f.send(await a.request('DELETE','/v1/host'));
  assert.equal((await f.store.get(a.id)).device_key_hash,null);
});

test('closed route surface, body limits and rate limits do not reach the provider', async t => {
  const f=await fixture(t);
  for(const path of ['/api/messages','/v1/host?auth=secret','/admin']) assert.equal((await f.send(new Request('https://connect.example.com'+path))).status,404);
  assert.equal((await f.send(new Request('http://connect.example.com/v1/hosts',{method:'POST'}))).status,400);
  assert.equal((await f.send(new Request('https://connect.example.com/v1/hosts',{method:'POST',body:'x'.repeat(9000)}))).status,413);
  f.env.REQUEST_LIMIT.limit=async()=>({success:false});
  assert.equal((await f.send(new Request('https://connect.example.com/v1/hosts',{method:'POST'}))).status,429);
  assert.equal(f.calls.length,0);
});

test('provider adapter fixes the loopback origin and refuses deletion of another DNS target', async () => {
  const { Provider }=await import('../service/connect/provider.mjs');
  const calls=[];
  const host={id:'a'.repeat(32),generation:1,tunnel_id:'owned',hostname:'c-owned.example.com'};
  let unrelated=false;
  const provider=new Provider({CF_API_TOKEN:'secret',CF_ACCOUNT_ID:'account',CF_ZONE_ID:'zone'},async (url,options)=>{
    calls.push({url,options});
    let result={id:'owned',name:'richos-'+host.id+'-g1'};
    if(url.includes('dns_records?')) result=[{id:'dns',type:'CNAME',content:unrelated?'someone-else.cfargotunnel.com':'owned.cfargotunnel.com'}];
    return Response.json({success:true,result});
  });
  await provider.configure(host);
  const config=JSON.parse(calls.find(call=>call.options.method==='PUT').options.body);
  assert.equal(config.config.ingress[0].service,'http://127.0.0.1:18443');
  assert.equal(config.config.ingress[1].service,'http_status:404');
  unrelated=true; await assert.rejects(provider.remove(host),/resource_conflict/);
  assert(!calls.some(call=>call.options.method==='DELETE'));
});

test('managed artifact preserves only server secrets and packages the tested service modules', async () => {
  const { managedArtifact } = await import('../cli/connect.mjs');
  const profile = { accountId:'a'.repeat(32),zoneId:'b'.repeat(32),databaseId:crypto.randomUUID(),domain:'example.com',capacity:10 };
  const artifact = managedArtifact(profile);
  assert.equal(artifact.metadata.main_module,'connect-worker.mjs');
  assert.deepEqual(artifact.metadata.keep_bindings,['secret_text']);
  assert.equal(artifact.metadata.bindings.find(row=>row.name==='ENROLLMENT_OPEN'),undefined,'no admission switch is uploaded');
  assert.equal(artifact.metadata.bindings.find(row=>row.name==='DB').id,profile.databaseId);
  assert.equal(artifact.modules.length,9);
  assert.deepEqual(artifact.metadata.bindings.find(row=>row.name==='AI'),{name:'AI',type:'ai'});
  assert(artifact.modules.every(row=>/^[a-f0-9]{64}$/.test(row.sha256)));
  // The 10-host lifetime ceiling was the private pilot (CEO 2026-10-06); the profile's number is 1000.
  assert.equal(managedArtifact({...profile,capacity:1000}).metadata.bindings.find(row=>row.name==='HOST_CAPACITY').text,'1000');
  assert.throws(()=>managedArtifact({...profile,capacity:0}),/capacity/);
  assert.throws(()=>managedArtifact({...profile,domain:'https://example.com'}),/Invalid/);
});

test('provider uses Worker-compatible manual redirects and rejects redirect responses', async () => {
  const { Provider } = await import('../service/connect/provider.mjs');
  const provider = new Provider({CF_API_TOKEN:'fixture'}, async (_url, options) => {
    assert.equal(options.redirect, 'manual');
    return new Response('', {status:302,headers:{location:'https://unrelated.example'}});
  });
  await assert.rejects(provider.api('/fixture'), /provider_unavailable/);
});

test('isolated host CLI signs control requests and keeps scoped credentials out of its result', async t => {
  const { lab } = await import('../dev/connect-lab.mjs');
  const { authenticate } = await import('../service/connect/auth.mjs');
  const fs = require('node:fs');
  const dir = require('./storage.cjs').createScratch('connect-host');
  t.after(() => fs.rmSync(dir,{recursive:true,force:true}));
  const identity = await lab('lab-identity',dir);
  assert.equal((await lab('lab-identity',dir)).id,identity.id);
  const transport = async (url, options) => {
    const request = new Request(url,options);
    const auth = await authenticate(request,options.body || '');
    assert.equal(auth.id,identity.id);
    return Response.json({id:identity.id,generation:1,enabled:options.method!=='DELETE',phase:'active',
      endpoint:`https://c-${identity.id}-g1.richos.ceo`,...(url.endsWith('/token')?{token:'scoped-fixture-token'}:{})});
  };
  const result = await lab('lab-enable',dir,transport);
  assert(!JSON.stringify(result).includes('scoped-fixture-token'));
  assert.equal(fs.statSync(join(dir,'tunnel-token')).mode & 0o777,0o600);
  await lab('lab-disable',dir,transport);
  assert(!fs.existsSync(join(dir,'tunnel-token')));
  await assert.rejects(lab('lab-identity','relative-lab-directory'),/storage path must be absolute/);
});



test('obsolete notification route names stay closed', async t => {
  const f=await fixture(t), a=await f.identity();
  for(const [method,path] of [['PUT','/v1/host/notification-device'],['POST','/v1/host/notifications'],['DELETE','/v1/host/notification-device']]) {
    assert.equal((await f.send(await a.request(method,path,'{}'))).status,404);
  }
  assert.equal(f.tunnels.size,0);
});


test('routing authenticates once, classifies without tunnels or stored content and fails safely', async t => {
  const f=await fixture(t), a=await f.identity();let calls=0;
  const state={user_request:'Investigate the three proposals',recent_conversation:'The proposals have not been read.',roles:{rich:'Conversation and quick lookups',stu:'Longer work and orchestration'}};
  let answer={model:'clef-flash',answers:{handler:{type:'choice',choice:'stu',probabilities:{rich:0.01,stu:0.98,uncertain:0.01}}}};
  f.env.AI={run:async(model,input)=>{calls++;assert.equal(model,'@cf/cloudflare/clef-flash');assert.deepEqual(input.state,state);return answer;}};
  const request=await a.request('POST','/v1/route',JSON.stringify(state));
  const unsigned=new Request(request.clone(),{headers:{}});assert.equal((await f.send(unsigned)).status,404);assert.equal(calls,0);
  let response=await f.send(request.clone());assert.equal(response.status,200);assert.deepEqual(await response.json(),{model:'clef-flash',handler:'stu'});
  assert.equal((await f.send(request)).status,409);assert.equal(calls,1,'nonce replay must not invoke the model twice');
  assert.equal(f.calls.length,0);assert.equal(f.tunnels.size,0);assert.equal(f.dns.size,0);
  const host=await f.store.get(a.id);assert.equal(host.desired,0);assert.equal(host.phase,'disabled');assert(!JSON.stringify(host).includes(state.user_request));
  for (const choice of ['rich','uncertain']) {
    answer.answers.handler.choice=choice;
    response=await f.send(await a.request('POST','/v1/route',JSON.stringify(state)));assert.equal(response.status,200);assert.equal((await response.json()).handler,choice);
  }
  answer={answers:{handler:{choice:'stu'}}};assert.equal((await f.send(await a.request('POST','/v1/route',JSON.stringify(state)))).status,503);
  f.env.AI.run=async()=>{throw Error('provider_unavailable')};assert.equal((await f.send(await a.request('POST','/v1/route',JSON.stringify(state)))).status,503);
  delete f.env.AI;assert.equal((await f.send(await a.request('POST','/v1/route',JSON.stringify(state)))).status,503);
  assert.equal((await f.send(await a.request('POST','/v1/route',JSON.stringify({...state,dispatch:true})))).status,400);
  assert.equal((await f.send(await a.request('POST','/v1/route','x'.repeat(20*1024+1)))).status,413);
  assert.equal((await f.send(await a.request('POST','/v1/hosts','x'.repeat(8193)))).status,413,'ordinary control limits stay unchanged');
});
