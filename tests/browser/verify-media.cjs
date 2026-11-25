// Actual decode + bidirectional data-channel check against the Rust synthetic peer.
// No desktop is captured and no local input is injected.
const fs = require('node:fs/promises');
const fssync = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
let playwright;
try { playwright = require('playwright'); }
catch(error) {
  if (!process.env.CODEX_PRIMARY_RUNTIME_NODE_MODULES) throw error;
  playwright = require(path.join(process.env.CODEX_PRIMARY_RUNTIME_NODE_MODULES,'playwright'));
}
const root = path.resolve(__dirname,'../..');
const binary = process.env.OPENREMOTE_SYNTHETIC_PEER || path.join(root,'agent-windows/target/debug/examples/synthetic_peer');
const ffmpeg = process.env.OPENREMOTE_FFMPEG || '/usr/bin/ffmpeg';
const chromium = process.env.OPENREMOTE_CHROMIUM || '/usr/bin/chromium';
const delay = ms => new Promise(resolve=>setTimeout(resolve,ms));
async function waitFor(fn, label, timeout=15000){
 const limit=Date.now()+timeout;
 while(Date.now()<limit){const value=await fn();if(value)return value;await delay(100);}
 throw new Error('Timed out: '+label);
}
async function stopProcess(child){
 if(!child||child.exitCode!==null)return;
 child.kill('SIGINT');
 await Promise.race([new Promise(r=>child.once('exit',r)),delay(3000)]);
 if(child.exitCode===null){child.kill('SIGKILL');await new Promise(r=>child.once('exit',r));}
}
async function launchRelay(folder){
 const secret=crypto.randomBytes(32).toString('hex');
 const child=spawn(path.join(root,'artifacts/test-relay'),[],{env:{...process.env,OPENREMOTE_TEST_TURN_SECRET:secret},stdio:['ignore','pipe','pipe']});
 let output='';let logs='';child.stdout.on('data',x=>output+=x);child.stderr.on('data',x=>logs+=x);
 child.on('error',e=>{logs+=String(e);});
 try{
  const config=await waitFor(()=>{if(child.exitCode!==null)throw new Error(logs);return output.includes('\n')?JSON.parse(output.split('\n')[0]):false;},'local TURN fixture');
  const username=Math.floor(Date.now()/1000+120)+':browser-test';
  const credential=crypto.createHmac('sha1',secret).update(username).digest('base64');
  return{child,config,username,credential};
 }catch(e){await stopProcess(child);throw e;}
}
async function oneMode(browser, mode, relay){
 const folder=await fs.mkdtemp(path.join(os.tmpdir(),'openremote-media-'));
 const context=await browser.newContext({viewport:{width:720,height:900}});
 const page=await context.newPage();
 const server=http.createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end('<!doctype html><html><body style="margin:0;background:#101827;color:white"><h1>OpenRemote synthetic video verification</h1><video id="remote" autoplay muted playsinline style="width:640px;height:360px"></video></body></html>');});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 let child;let logs='';
 try{
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  const ice=mode==='direct'?[]:[{urls:[mode==='relay-udp'?relay.config.udpURL:relay.config.tcpURL],username:relay.username,credential:relay.credential}];
  const offer=await page.evaluate(async ({ice,mode})=>{
   const pc=new RTCPeerConnection({iceServers:ice,iceTransportPolicy:mode==='direct'?'all':'relay'});
   const dc=pc.createDataChannel('control',{ordered:true});
   const state={pc,dc,frames:0,errors:[],capabilities:null};window.remoteTest=state;
   const trans=pc.addTransceiver('video',{direction:'recvonly'});
   const h264=RTCRtpReceiver.getCapabilities('video').codecs.filter(c=>c.mimeType.toLowerCase()==='video/h264');
   if(h264.length)trans.setCodecPreferences(h264);
   pc.ontrack=({track,streams})=>{const video=document.getElementById('remote');video.srcObject=streams[0]||new MediaStream([track]);video.play().catch(e=>state.errors.push(String(e)));const frame=()=>{state.frames++;video.requestVideoFrameCallback(frame);};video.requestVideoFrameCallback(frame);};
   dc.onmessage=e=>{try{state.capabilities=JSON.parse(e.data);}catch{state.errors.push('invalid host message');}};
   await pc.setLocalDescription(await pc.createOffer());
   if(pc.iceGatheringState!=='complete')await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(new Error('browser ICE timeout')),15000);
    pc.addEventListener('icegatheringstatechange',()=>{if(pc.iceGatheringState==='complete'){clearTimeout(timer);resolve();}});
   });
   return{type:pc.localDescription.type,sdp:pc.localDescription.sdp};
  },{ice,mode});
  const offerFile=path.join(folder,'offer.json'),answerFile=path.join(folder,'answer.json'),eventsFile=path.join(folder,'events.jsonl');
  await fs.writeFile(offerFile,JSON.stringify(offer));
  child=spawn(binary,['--offer-file',offerFile,'--answer-file',answerFile,'--events-file',eventsFile,'--ffmpeg',ffmpeg,'--seconds','45'],{stdio:['ignore','pipe','pipe']});
  child.stdout.on('data',x=>logs+=x);child.stderr.on('data',x=>logs+=x);child.on('error',e=>logs+=String(e));
  await waitFor(()=>{if(child.exitCode!==null)throw new Error('Rust peer exited: '+logs);return fssync.existsSync(answerFile);},'complete Rust answer',25000);
  const answer=JSON.parse(await fs.readFile(answerFile,'utf8'));
  await page.evaluate(answer=>window.remoteTest.pc.setRemoteDescription(answer),answer);
  await page.waitForFunction(()=>window.remoteTest.pc.connectionState==='connected'&&window.remoteTest.dc.readyState==='open',null,{timeout:20000});
  await page.waitForFunction(()=>window.remoteTest.frames>=10&&document.getElementById('remote').videoWidth===640,null,{timeout:20000});
  const capture=()=>page.evaluate(()=>{
   const video=document.getElementById('remote');const canvas=document.createElement('canvas');canvas.width=160;canvas.height=90;
   const c=canvas.getContext('2d');c.drawImage(video,0,0,160,90);const data=c.getImageData(0,0,160,90).data;
   let hash=2166136261;for(let i=0;i<data.length;i++){hash^=data[i];hash=Math.imul(hash,16777619);}return hash>>>0;
  });
  const first=await capture();await delay(1300);const second=await capture();
  if(first===second)throw new Error('Decoded frames are not visibly changing');
  const expected=[{v:1,type:'pointer',action:'move',x:0.25,y:0.75,buttons:0},{v:1,type:'text',text:'OpenRemote తెలుగు ✓'},{v:1,type:'release_all'}];
  await page.evaluate(events=>events.forEach(e=>window.remoteTest.dc.send(JSON.stringify(e))),expected);
  const received=await waitFor(async()=>{const raw=await fs.readFile(eventsFile,'utf8');const a=raw.trim().split('\n').filter(Boolean).map(x=>JSON.parse(x));return a.length>=expected.length?a:false;},'host input receipts');
  if(JSON.stringify(received)!==JSON.stringify(expected))throw new Error('Control messages changed in transit');
  const metrics=await page.evaluate(async()=>{
   const s=window.remoteTest;const stats=await s.pc.getStats();let video=null,pair=null,codec=null,local=null;
   for(const r of stats.values()){if(r.type==='inbound-rtp'&&r.kind==='video'){video=r;codec=stats.get(r.codecId);}if(r.type==='transport'&&r.selectedCandidatePairId)pair=stats.get(r.selectedCandidatePairId);}
   if(pair)local=stats.get(pair.localCandidateId);
   return{connection:s.pc.connectionState,framesDisplayed:s.frames,framesDecoded:video?.framesDecoded,bytesReceived:video?.bytesReceived,codec:codec?.mimeType,localCandidateType:local?.candidateType,relayProtocol:local?.relayProtocol,dimensions:[document.getElementById('remote').videoWidth,document.getElementById('remote').videoHeight],hostCapabilities:s.capabilities,errors:s.errors};
  });
  if(metrics.codec?.toLowerCase()!=='video/h264'||metrics.framesDecoded<10)throw new Error('H.264 not actually decoded');
  if(mode!=='direct'&&metrics.localCandidateType!=='relay')throw new Error('Forced relay test did not select a relay candidate');
  await page.evaluate(()=>window.remoteTest.pc.close());
  const result={mode,ok:true,...metrics,movingPixels:true,controlEventsReceived:received.length,scope:'Synthetic source + production Rust peer/media parser + Chromium decoder; no Windows capture or injection'};
  return result;
 }catch(e){e.message+='\nPeer log:\n'+logs;throw e;}
 finally{await stopProcess(child);await context.close();await new Promise(resolve=>server.close(resolve));await fs.rm(folder,{recursive:true,force:true});}
}
(async()=>{
 let browser,relay;
 try{
  browser=await playwright.chromium.launch({executablePath:chromium,headless:true,args:['--autoplay-policy=no-user-gesture-required']});
  const results=[];
  results.push(await oneMode(browser,'direct',null));
  if(!process.env.OPENREMOTE_SKIP_RELAY){relay=await launchRelay();for(const mode of ['relay-udp','relay-tcp'])results.push(await oneMode(browser,mode,relay));}
  const report={ok:true,browser:await browser.version(),tests:results};
  await fs.mkdir(path.join(root,'evidence'),{recursive:true});
  await fs.writeFile(path.join(root,'evidence/media-browser.json'),JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify(report,null,2));
 }catch(e){console.error(e);process.exitCode=1;}
 finally{await browser?.close();await stopProcess(relay?.child);}
})();
