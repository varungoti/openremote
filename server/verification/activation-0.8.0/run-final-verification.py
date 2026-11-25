#!/usr/bin/env python3
"""Record exact coordinator source/test/build evidence without modifying source."""
import hashlib,json,os,pathlib,shutil,subprocess,time
from datetime import datetime,timezone
ROOT=pathlib.Path(__file__).resolve().parents[3]
SERVER=ROOT/"server"
OUT=pathlib.Path(__file__).resolve().parent
GO=pathlib.Path("/workspace/scratch/72f8c6db0182/toolchains/go1.27.4/go/bin/go")
BIN=ROOT/"artifacts/controlplane-0.8.0-linux-amd64"
ENV={**os.environ,"GOTOOLCHAIN":"local"}
def now(): return datetime.now(timezone.utc).isoformat()
def write(name,value): (OUT/name).write_text(json.dumps(value,indent=2)+"\n")
def pins():
    paths=sorted(SERVER.rglob("*.go"))+[SERVER/"go.mod"]+[SERVER/"internal/webclient/assets"/name for name in ["index.html","app.js","core.mjs","style.css"]]
    return {str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths if "verification" not in p.parts}
def output(cmd): return subprocess.check_output(cmd,cwd=SERVER,env=ENV,text=True).strip()
if (OUT/"final-verification.json").exists(): raise SystemExit("Final evidence exists; archive it before any rerun")
before=pins(); write("final-source-inputs-before.json",before)
meta={"startedAt":now(),"sourceRevision":output(["git","rev-parse","HEAD"]),"sourceBranch":output(["git","branch","--show-current"]),"sourceScope":"Exact uncommitted source input hashes are authoritative; base revision is contextual.","goVersion":output([str(GO),"version"]),"goEnvironment":json.loads(output([str(GO),"env","-json","GOOS","GOARCH","CGO_ENABLED","GOVERSION"])),"toolchainProvenance":"go-toolchain-provenance.json","sourceInputsBefore":"final-source-inputs-before.json","checks":[]}
provenance=ROOT/"evidence/build-tools-0.8.0/restored-downloads.json"
shutil.copyfile(provenance,OUT/"go-toolchain-provenance.json")
write("final-verification.json",meta)
def run(name,args,env=None):
    log=OUT/("final-"+name+".log")
    cmd=[str(GO),*args]
    start=time.monotonic()
    with log.open("w") as stream:
        result=subprocess.run(cmd,cwd=SERVER,env={**ENV,**(env or {})},stdout=stream,stderr=subprocess.STDOUT)
    row={"name":name,"command":cmd,"cwd":str(SERVER),"environment":{"GOTOOLCHAIN":"local",**(env or {})},"exitCode":result.returncode,"durationSeconds":round(time.monotonic()-start,3),"log":log.name}
    if name in ("unit","race"):
        events=[]
        for line in log.read_text().splitlines():
            try: events.append(json.loads(line))
            except ValueError: pass
        passed=[v for v in events if v.get("Action")=="pass" and v.get("Test")]
        row.update(topLevelTestsPassed=sum("/" not in v["Test"] for v in passed),testsIncludingSubcasesPassed=len(passed),activationTopLevelTestsPassed=sum(v["Test"].startswith("TestActivation") and "/" not in v["Test"] for v in passed),packagesPassed=[v["Package"] for v in events if v.get("Action")=="pass" and not v.get("Test")],failures=[v for v in events if v.get("Action")=="fail"])
    meta["checks"].append(row); write("final-verification.json",meta)
    print(json.dumps(row),flush=True)
    if result.returncode: raise SystemExit(result.returncode)
run("unit",["test","-count=1","-json","./..."])
run("vet",["vet","./..."])
run("build",["build","-trimpath","-ldflags=-s -w","-o",str(BIN),"./cmd/controlplane"],{"CGO_ENABLED":"0","GOOS":"linux","GOARCH":"amd64"})
headers=output(["readelf","-l",str(BIN)])
file_description=output(["file",str(BIN)])
build_metadata=output([str(GO),"version","-m",str(BIN)])
(OUT/"final-binary-program-headers.log").write_text(headers+"\n")
(OUT/"final-binary-file.log").write_text(file_description+"\n")
(OUT/"final-binary-go-metadata.log").write_text(build_metadata+"\n")
if "INTERP" in headers or "CGO_ENABLED=0" not in build_metadata: raise SystemExit("Static linking evidence failed")
meta["artifact"]={"path":str(BIN.relative_to(ROOT)),"sha256":hashlib.sha256(BIN.read_bytes()).hexdigest(),"size":BIN.stat().st_size,"format":file_description,"staticLinkVerifiedBy":"readelf -l has no PT_INTERP; Go build metadata CGO_ENABLED=0"}
write("final-build.json",{"artifact":meta["artifact"],"buildCheck":meta["checks"][-1],"sourceInputs":before,"recordedAt":now()})
write("final-verification.json",meta)
print(json.dumps({"binaryReady":meta["artifact"]}),flush=True)
run("race",["test","-race","-count=1","-json","./..."],{"CGO_ENABLED":"1","GOOS":"linux","GOARCH":"amd64"})
after=pins(); write("final-source-inputs-after.json",after)
meta.update(completedAt=now(),inputsStillMatch=before==after,sourceInputsAfter="final-source-inputs-after.json",sourceInputs=after,artifactHashStillMatches=meta["artifact"]["sha256"]==hashlib.sha256(BIN.read_bytes()).hexdigest(),limitations=["These checks do not establish native Windows UI, physical phone, browser media decode or real Internet TURN execution.","Root-owned HTTPS integration and frontend VM checks have separate evidence.","Activation requires an operator-deployed service and a publisher-configured origin; no public service deployment or agent desktop execution is claimed."])
write("final-verification.json",meta)
if not meta["inputsStillMatch"] or not meta["artifactHashStillMatches"]: raise SystemExit("Source or artifact changed during final verification")
old=SERVER/"BUILD_EVIDENCE.json"
archive=SERVER/"verification/history/0.7.0/BUILD_EVIDENCE.json"
archive.parent.mkdir(parents=True,exist_ok=True)
if old.exists() and not archive.exists(): shutil.copyfile(old,archive)
meta["verificationStatus"]="Final normal Go suite, race suite and vet passed; static coordinator build and source/artifact custody verified. Separate device/network release gates remain."
meta["evidenceDirectory"]="verification/activation-0.8.0"
write("final-summary.json",meta)
old.write_text(json.dumps(meta,indent=2)+"\n")
print(json.dumps({"complete":True,"sourceInputs":len(before),"artifact":meta["artifact"]}),flush=True)
