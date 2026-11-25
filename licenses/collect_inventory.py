#!/usr/bin/env python3
"""Collect declared metadata and existing license bytes; never infer missing licenses.
The result is a review inventory, not an assertion of redistribution compliance.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import tomllib
import xml.etree.ElementTree as ET
import zipfile
from datetime import datetime, timezone

DATE = datetime.now(timezone.utc).date().isoformat()
NS = {'m':'http://maven.apache.org/POM/4.0.0'}

def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1024*1024),b''): h.update(chunk)
    return h.hexdigest()

def copy_record(source,target,root):
    target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(source,target)
    return {'file':str(target.relative_to(root)),'sha256':digest(target),'size_bytes':target.stat().st_size}

def pom_licenses(path):
    xml=ET.parse(path).getroot()
    return [{'name':n.findtext('m:name',default='',namespaces=NS),'url':n.findtext('m:url',default='',namespaces=NS)} for n in xml.findall('m:licenses/m:license',NS)]

def android_runtime(report,cache,out,root):
    result={'status':'report_unavailable','scope':'Selected components in the Gradle releaseRuntimeClasspath report, including platforms/metadata modules; not a final signed APK binary SBOM','components':[]}
    if not report.is_file():return result
    source=report.read_text()
    if 'BUILD SUCCESSFUL' not in source or re.search(r'\bFAILED\b',source):
        result['status']='report_not_successful';return result
    coordinates=set()
    for line in source.splitlines():
        match=re.search(r'(?:\+---|\\---) ([^\s]+)(?: -> ([^\s]+))?',line)
        if not match or '(c)' in line:continue
        parts=match[1].split(':')
        if len(parts)<2:raise ValueError('Unrecognized Gradle runtime component: '+line)
        version=match[2] or (parts[2] if len(parts)>2 else None)
        if not version:raise ValueError('Unresolved Gradle runtime component: '+line)
        coordinates.add((parts[0],parts[1],version))
    if not coordinates:raise ValueError('No resolved components in runtime report')
    result.update(status='successful_resolved_report_recorded',report=copy_record(report,out/'android/releaseRuntimeClasspath.txt',root),selection_method='Parse Gradle selected versions after arrows; deduplicate repeated subtrees; exclude constraint-only (c) rows',components_count=len(coordinates))
    def notices(z,target):
        files=[]
        for name in z.namelist():
            path=PurePosixPath(name)
            if path.is_absolute() or '..' in path.parts or not path.name:continue
            if not path.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE','PATENTS')):continue
            info=z.getinfo(name)
            if info.is_dir() or info.file_size>2*1024*1024:continue
            data=z.read(name);dest=target.joinpath(*path.parts);dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
            files.append({'archive_entry':name,'file':str(dest.relative_to(root)),'sha256':hashlib.sha256(data).hexdigest(),'size_bytes':len(data)})
        return files
    for group,artifact,version in sorted(coordinates):
        row={'coordinate':f'{group}:{artifact}:{version}','group':group,'artifact':artifact,'version':version,'metadata_status':'not_found','reported_licenses':[],'cached_artifacts':[]}
        folder=cache/group/artifact/version;poms=list(folder.glob('**/*.pom'))
        if len(poms)==1:
            pom=poms[0];row.update(metadata_status='exact_cached_POM',reported_licenses=pom_licenses(pom),pom=copy_record(pom,out/'android/poms'/f'{group}-{artifact}-{version}.pom',root))
            repo='https://dl.google.com/dl/android/maven2/' if group.startswith('androidx.') else 'https://repo.maven.apache.org/maven2/'
            row['metadata_url']=repo+group.replace('.','/')+'/'+artifact+'/'+version+'/'+artifact+'-'+version+'.pom'
            row['license_declaration_source']=row['coordinate'] if row['reported_licenses'] else None
            if not row['reported_licenses']:
                parent=ET.parse(pom).getroot().find('m:parent',NS)
                if parent is not None:
                    pg=parent.findtext('m:groupId',namespaces=NS);pa=parent.findtext('m:artifactId',namespaces=NS);pv=parent.findtext('m:version',namespaces=NS)
                    if pg and pa and pv:
                        pp=list((cache/pg/pa/pv).glob('**/*.pom'))
                        if len(pp)==1:
                            row['parent_pom']=copy_record(pp[0],out/'android/poms'/f'{pg}-{pa}-{pv}.pom',root)
                            row['parent_metadata_url']='https://repo.maven.apache.org/maven2/'+pg.replace('.','/')+'/'+pa+'/'+pv+'/'+pa+'-'+pv+'.pom'
                            row['reported_licenses']=pom_licenses(pp[0])
                            row['license_declaration_source']=f'{pg}:{pa}:{pv}'
                            row['license_declaration_note']='Component POM declares no licenses; these declarations were read from its exact named parent POM.'
        for archive in sorted(list(folder.glob('**/*.aar'))+list(folder.glob('**/*.jar'))):
            record={'filename':archive.name,'sha256':digest(archive),'size_bytes':archive.stat().st_size,'copied_notices':[]}
            target=out/'android/notices'/f'{group}-{artifact}-{version}'/archive.name
            with zipfile.ZipFile(archive) as z:
                record['copied_notices']=notices(z,target)
                if archive.suffix=='.aar' and 'classes.jar' in z.namelist():
                    with zipfile.ZipFile(io.BytesIO(z.read('classes.jar'))) as nested:
                        nested_files=notices(nested,target/'classes.jar')
                        for item in nested_files:item['archive_entry']='classes.jar!/'+item['archive_entry']
                        record['copied_notices']+=nested_files
            row['cached_artifacts'].append(record)
        row['artifact_scope']='Cached AAR/JAR hashes; modules without such artifacts may be metadata/platform components. This does not establish final APK inclusion.'
        result['components'].append(row)
    return result

def android_integrity(root,runtime):
    """Bind this inventory to existing Gradle inputs; do not generate/change pins."""
    def record(path):
        return {'file':str(path.relative_to(root)),'sha256':digest(path),'size_bytes':path.stat().st_size}
    inputs=['android/settings.gradle.kts','android/build.gradle.kts','android/app/build.gradle.kts',
            'android/gradle.properties','android/gradle/wrapper/gradle-wrapper.properties',
            'android/app/gradle.lockfile','android/gradle/verification-metadata.xml']
    result={'scope':'Existing app lock and Gradle verification metadata; build/test/plugin components are not thereby a production license inventory',
            'source_inputs':[record(root/name) for name in inputs if (root/name).is_file()]}
    lock=root/'android/app/gradle.lockfile'
    if lock.is_file():
        configurations=set();coordinates=set();locked_runtime=set()
        for line in lock.read_text().splitlines():
            if not line or line.startswith('#'):continue
            coordinate,scopes=line.split('=',1);scopes=set(filter(None,scopes.split(',')));configurations.update(scopes)
            if coordinate!='empty':coordinates.add(coordinate)
            if 'releaseRuntimeClasspath' in scopes:locked_runtime.add(coordinate)
        reported={x['coordinate'] for x in runtime['components']}
        result['app_lock']={'configurations_count':len(configurations),'components_count':len(coordinates),'release_runtime_components_count':len(locked_runtime),
                            'runtime_report_matches_lock':reported==locked_runtime}
        if runtime['status']=='successful_resolved_report_recorded' and reported!=locked_runtime:
            raise ValueError('Runtime report differs from final app lockfile')
    metadata=root/'android/gradle/verification-metadata.xml'
    if metadata.is_file():
        ns={'v':'https://schema.gradle.org/dependency-verification'};xml=ET.parse(metadata).getroot()
        components=xml.findall('v:components/v:component',ns);artifacts={}
        for component in components:
            coordinate=':'.join(component.attrib[x] for x in ('group','name','version'))
            for artifact in component.findall('v:artifact',ns):
                hashes=set()
                for checksum in artifact.findall('v:sha256',ns):
                    hashes.add(checksum.attrib['value'])
                    hashes.update(x.attrib['value'] for x in checksum.findall('v:also-trust',ns))
                artifacts[(coordinate,artifact.attrib['name'])]=hashes
        checked=0;unlisted=[]
        for component in runtime['components']:
            for artifact in component['cached_artifacts']:
                known=artifacts.get((component['coordinate'],artifact['filename']))
                artifact['gradle_verification_sha256_match']=artifact['sha256'] in known if known else None
                if known:
                    if artifact['sha256'] not in known:raise ValueError('Runtime artifact differs from Gradle verification hash: '+artifact['filename'])
                    checked+=1
                else:unlisted.append(component['coordinate']+'/'+artifact['filename'])
        result['verification_metadata']={'components_count':len(components),'artifacts_count':len(artifacts),'runtime_archives_matching_sha256':checked,'runtime_archives_without_hash_entry':unlisted,
             'verify_signatures':xml.findtext('v:configuration/v:verify-signatures',namespaces=ns)=='true',
             'provenance_limit':'These existing first-seen SHA-256 pins are not publisher signatures, attestations, license clearance, or an independently reproduced build.'}
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[1])
    p.add_argument('--cargo-source',type=Path,required=True)
    p.add_argument('--gradle-cache',type=Path,required=True)
    p.add_argument('--go-root',type=Path,required=True)
    p.add_argument('--android-runtime-report',type=Path)
    args=p.parse_args();root=args.root.resolve();out=root/'licenses';out.mkdir(exist_ok=True)
    upstream_path=out/'rust-upstream/provenance.json'
    upstream_rows=json.loads(upstream_path.read_text()) if upstream_path.exists() else []
    upstream_by_package={(r['name'],r['version']):r for r in upstream_rows}
    lock=root/'agent-windows/Cargo.lock'
    packages=tomllib.loads(lock.read_text())['package'];rust=[]
    for item in packages:
        if 'source' not in item: continue
        name,version=item['name'],item['version']
        row={'name':name,'version':version,'source':item['source'],'lock_checksum':item.get('checksum'),'crate_url':f'https://crates.io/crates/{name}/{version}','metadata_status':'missing','declared_license':None,'copied_notices':[]}
        candidates=list(args.cargo_source.glob(f'*/{name}-{version}'))
        if len(candidates)!=1:
            row['reason']='exact cached source missing or ambiguous';rust.append(row);continue
        source=candidates[0];metadata=source/'Cargo.toml'
        meta=tomllib.loads(metadata.read_text())['package']
        row.update(metadata_status='read_exact_cached_crate',metadata_sha256=digest(metadata),declared_license=meta.get('license'),declared_license_file=meta.get('license-file'),repository=meta.get('repository'),authors=meta.get('authors',[]))
        archive=args.cargo_source.parent/'cache'/source.parent.name/f'{name}-{version}.crate'
        if archive.exists():
            row['cached_archive_sha256']=digest(archive)
            row['cached_archive_checksum_matches_lock']=row['cached_archive_sha256']==item.get('checksum')
        checksum=source/'.cargo-checksum.json' 
        if checksum.exists():
            v=json.loads(checksum.read_text());row['cache_package_checksum_matches_lock']=v.get('package')==item.get('checksum')
        files=[]
        for f in source.rglob('*'):
            if not f.is_file() or f.is_symlink():continue
            n=f.name.upper()
            if n.startswith(('LICENSE','LICENCE','COPYING','NOTICE','PATENTS')) and f.stat().st_size<=2*1024*1024:files.append(f)
            elif n=='AUTHORS' and f.stat().st_size<=2*1024*1024:
                # r-efi carries its actual MIT text and copyright in AUTHORS.
                # Preserve an AUTHORS file only when it identifies license terms.
                text=f.read_text(errors='replace')
                if 'LICENSE:' in text and 'Permission is hereby granted' in text:files.append(f)
        custom=meta.get('license-file')
        if isinstance(custom,str):
            f=(source/custom).resolve()
            if f.is_relative_to(source.resolve()) and f.is_file() and not f.is_symlink() and f not in files:files.append(f)
        for f in sorted(set(files)):
            row['copied_notices'].append(copy_record(f,out/'rust'/f'{name}-{version}'/f.relative_to(source),root))
        row['upstream_notices']=[]
        if (name,version) in upstream_by_package:
            evidence=upstream_by_package[(name,version)]
            if evidence['crate_archive_sha256']!=row['lock_checksum']:raise ValueError('Upstream provenance refers to another crate archive')
            row['upstream_provenance_status']=evidence['status']
            row['upstream_source_commit']=evidence.get('source_commit')
            if 'scope_note' in evidence:row['upstream_notice_scope_note']=evidence['scope_note']
            for notice in evidence['notices']:
                if 'file' not in notice:continue
                if digest(root/notice['file'])!=notice['sha256']:raise ValueError('Upstream notice changed: '+notice['file'])
                row['upstream_notices'].append(notice)
        row['notice_status']='available_files_copied_not_legal_reviewed' if files else 'no_license_or_notice_file_found_in_cached_crate'
        if not files and row['upstream_notices']:row['notice_status']='exact_commit_upstream_texts_copied_not_legal_reviewed'
        rust.append(row)
    rust.sort(key=lambda r:(r['name'],r['version']))
    go_license=copy_record(args.go_root/'LICENSE',out/'go/GO-LICENSE.txt',root)
    go={'module':'openremote/controlplane','go_mod_sha256':digest(root/'server/go.mod'),'third_party_modules':[],'stdlib_source':'official Go toolchain; downloaded toolchain provenance in project evidence','toolchain_version':(args.go_root/'VERSION').read_text().splitlines()[0],'license':go_license,'scope':'production module only; Pion test harness modules are excluded from this runtime inventory'}
    cache=args.gradle_cache
    bompath=next(iter(cache.glob('androidx.compose/compose-bom/2025.10.01/*/*.pom')),None)
    bom={}
    if bompath:
        for d in ET.parse(bompath).getroot().findall('m:dependencyManagement/m:dependencies/m:dependency',NS):
            group=d.findtext('m:groupId',namespaces=NS);artifact=d.findtext('m:artifactId',namespaces=NS);version=d.findtext('m:version',namespaces=NS)
            if group and artifact and version:bom[(group,artifact)]=version
    android=[]
    pattern=r'(implementation|testImplementation|androidTestImplementation|debugImplementation)\((?:platform\()?"([^"]+)"'
    for scope,coordinate in re.findall(pattern,(root/'android/app/build.gradle.kts').read_text()):
        parts=coordinate.split(':');group,artifact=parts[:2];version=parts[2] if len(parts)>2 else bom.get((group,artifact))
        row={'scope':scope,'declared_coordinate':coordinate,'version':version,'version_source':'explicit' if len(parts)>2 else 'Compose BOM2025.10.01' if version else 'unresolved','reported_licenses':[],'metadata_status':'not_found'}
        if version:
            poms=list((cache/group/artifact/version).glob('**/*.pom'))
            if len(poms)==1:
                pom=poms[0];row['reported_licenses']=pom_licenses(pom);row['pom']=copy_record(pom,out/'android/poms'/f'{group}-{artifact}-{version}.pom',root);row['metadata_status']='exact_cached_POM'
                repository='https://dl.google.com/dl/android/maven2/' if group.startswith('androidx.') else 'https://repo.maven.apache.org/maven2/'
                row['metadata_url']=repository+group.replace('.','/')+'/'+artifact+'/'+version+'/'+artifact+'-'+version+'.pom'
        android.append(row)
    runtime=android_runtime(args.android_runtime_report or root/'android/verification/release-runtime-dependencies.txt',cache,out,root)
    integrity=android_integrity(root,runtime)
    native={'coordinate':'io.github.webrtc-sdk:android:150.7871.04','complete_notice_status':'BLOCKED: native source/dependency build provenance and complete per-binary notices are unavailable','artifact_notice_entries':[],'native_libraries':[]}
    aars=list((cache/'io.github.webrtc-sdk/android/150.7871.04').glob('**/*.aar'))
    if len(aars)==1:
        aar=aars[0];native.update(aar_sha256=digest(aar),aar_size_bytes=aar.stat().st_size,artifact_url='https://repo.maven.apache.org/maven2/io/github/webrtc-sdk/android/150.7871.04/android-150.7871.04.aar')
        with zipfile.ZipFile(aar) as z:
            native['artifact_notice_entries']=[n for n in z.namelist() if any(x in n.lower() for x in ('license','notice','copying','patents'))]
            if 'classes.jar' in z.namelist():
                with zipfile.ZipFile(io.BytesIO(z.read('classes.jar'))) as j:
                    native['classes_jar_notice_entries']=[n for n in j.namelist() if any(x in n.lower() for x in ('license','notice','copying','patents'))]
            for name in z.namelist():
                if name.endswith('.so'):
                    h=hashlib.sha256()
                    with z.open(name) as f:
                        for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk)
                    native['native_libraries'].append({'entry':name,'sha256':h.hexdigest(),'size_bytes':z.getinfo(name).file_size})
    inventory={'checked_at':DATE,'purpose':'Declared-license and available-file inventory; public-redistribution gate remains blocked','rust_lock_sha256':digest(lock),'rust_scope':'All locked registry packages including build/dev/target-specific packages, not an exact linked Windows binary SBOM','rust_packages':rust,'go_server':go,'android_scope':'Direct declarations plus selected transitive components in the captured successful releaseRuntimeClasspath report; not a final signed APK binary SBOM','android_declared_dependencies':android,'android_release_runtime':runtime,'android_build_plugins':re.findall(r'id\("([^"]+)"\) version "([^"]+)"',(root/'android/build.gradle.kts').read_text()),'webrtc_native':native,'unresolved_gates':['Complete native Android WebRTC transitive license/notice bundle tied to this exact AAR hash and upstream build revision','Bind the recorded Android runtime graph to final signed artifacts, collect/review remaining component notices, and inventory Rust final target/features','Review per-file source headers/vendor components and any license-file gaps; copied metadata is not a legal/compliance conclusion','Redistributed FFmpeg executable provenance, configure flags, dependency notices and corresponding source/license materials; no Windows FFmpeg binary is bundled here','Test-only Pion/fixture and toolchain dependencies are not exhaustively inventoried by this production inventory']}
    inventory['rust_project_version']=next((x['version'] for x in packages if x['name']=='openremote-agent' and 'source' not in x),None)
    inventory['android_dependency_integrity']=integrity
    if upstream_path.exists():inventory['rust_upstream_provenance']={'file':str(upstream_path.relative_to(root)),'sha256':digest(upstream_path),'size_bytes':upstream_path.stat().st_size}
    (out/'dependency-inventory.json').write_text(json.dumps(inventory,indent=2,ensure_ascii=False)+'\n')
    missing=[r for r in rust if r['metadata_status']=='missing'];no_files=[r for r in rust if not r['copied_notices'] and not r.get('upstream_notices')]
    lines=['# Dependency license inventory','',f'Checked: {DATE}. This inventory records metadata and copied license bytes; redistribution review is **BLOCKED**, not approved.','',f'Rust: **{len(rust)}** locked registry packages; **{len(rust)-len(missing)}** exact cached manifests read; **{sum(len(r["copied_notices"]) for r in rust)}** cached license/notice files and **{sum(len(r.get("upstream_notices",[])) for r in rust)}** exact-commit upstream text copies. **{len(no_files)}** packages still have no copied notice text.','', 'Scope includes target/build/dev packages from Cargo.lock and does not claim they all ship in the Windows binary. Upstream provenance is in `rust-upstream/provenance.json`; the raw declared license alternatives are preserved without choosing one.','', '| Package | Version | Metadata license expression | Cached / upstream notice files |','| --- | --- | --- | --- |']
    for r in rust:lines.append(f'| {r["name"]} | {r["version"]} | {str(r["declared_license"] or "UNDECLARED / UNAVAILABLE").replace("|","/")} | {len(r["copied_notices"])} / {len(r.get("upstream_notices",[]))} |')
    lines+=['','## Android direct declarations','', '| Scope | Declared coordinate | Selected version | POM-reported licenses |','| --- | --- | --- | --- |']
    for r in android:lines.append(f'| {r["scope"]} | {r["declared_coordinate"]} | {r["version"] or "UNRESOLVED"} | {", ".join(x["name"] for x in r["reported_licenses"]) or "NOT VERIFIED"} |')
    lines+=['','## Android selected release runtime components','',f'Report status: `{runtime["status"]}`. Includes selected transitive components and platforms/metadata modules; constraint-only rows are excluded. Artifact hashes and available embedded notice copies are recorded in JSON. This is not a complete native notice bundle or a final signed APK SBOM.','', '| Selected component | POM-reported licenses | Cached AAR/JAR files | Copied embedded notices |','| --- | --- | --- | --- |']
    for r in runtime['components']:lines.append(f'| {r["coordinate"]} | {", ".join(x["name"] for x in r["reported_licenses"]) or "NOT VERIFIED"} | {len(r["cached_artifacts"])} | {sum(len(a["copied_notices"]) for a in r["cached_artifacts"])} |')
    if 'app_lock' in integrity and 'verification_metadata' in integrity:
        locked=integrity['app_lock'];verified=integrity['verification_metadata']
        lines+=['','## Gradle source and integrity bindings','',f'The captured app lock contains **{locked["configurations_count"]} configurations** and **{locked["components_count"]} components**. Its **{locked["release_runtime_components_count"]} release runtime components** match the report. Existing Gradle verification metadata contains **{verified["components_count"]} components / {verified["artifacts_count"]} artifacts** across build scopes; **{verified["runtime_archives_matching_sha256"]} cached runtime AAR/JAR hashes** match those pins.','', 'Exact build-script, lockfile and verification-metadata hashes are recorded in JSON. These are existing first-seen provenance pins, not publisher signatures, a complete production license inventory, or redistribution approval.','']
    lines+=['','## Outstanding release-notice gates','']+['- '+x for x in inventory['unresolved_gates']]+['','## Sources and integrity','', 'The JSON records per-file SHA-256 hashes, source archive checksums, POM source URLs, artifact hashes, and the exact Cargo.lock hash. License text copies are under `licenses/rust`, `licenses/rust-upstream`, `licenses/go`, and `licenses/upstream`. Upstream downloads have separate provenance manifests.','']
    (out/'dependency-inventory.md').write_text('\n'.join(lines))
    print(json.dumps({'rust_locked':len(rust),'manifest_missing':len(missing),'no_license_files':len(no_files),'copied_files':sum(len(r['copied_notices']) for r in rust),'upstream_rust_files':sum(len(r.get('upstream_notices',[])) for r in rust),'android_declared':len(android),'android_runtime_components':len(runtime['components']),'android_embedded_notices':sum(len(a['copied_notices']) for r in runtime['components'] for a in r['cached_artifacts']),'native_notices':native['artifact_notice_entries'],'native_libraries':len(native['native_libraries'])}))

if __name__=='__main__':main()
