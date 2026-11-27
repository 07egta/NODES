from pathlib import Path
import hashlib, json, os, shutil, subprocess, time
from datetime import datetime, timezone

ROOT=Path('/workspace/scratch/74d3da15ab33')
WORK=ROOT/'NODES-diagnostics-cancellation-cost'
EVIDENCE=ROOT/'NODES/docs/development/diagnostics-cancellation-cost'
TARGET=ROOT/'diagnostics-bench-target'
CARGO='/home/agent/.cargo/bin/cargo'
RUSTC='/home/agent/.cargo/bin/rustc'
CAPTURE=json.loads((EVIDENCE/'source-capture.json').read_text())
SOURCES={item['label']:item for item in CAPTURE['snapshots']}

def now(): return datetime.now(timezone.utc).isoformat()
def sha(path):
    with path.open('rb') as stream: return hashlib.file_digest(stream,'sha256').hexdigest()
def fnv(path):
    value=0xcbf29ce484222325
    for byte in path.read_bytes(): value=((value^byte)*0x100000001b3)&0xffffffffffffffff
    return f'{value:016x}'
def command_output(command): return subprocess.check_output(command,cwd=WORK,text=True).strip()

for record in SOURCES.values():
    assert sha(WORK/'benchmark-snapshots'/record['snapshot'])==record['sha256']
    assert (WORK/'benchmark-snapshots'/record['snapshot']).read_bytes()==subprocess.check_output(['git','show',record['revision']+':src/diagnostics.rs'],cwd=WORK)
    record['fnv1a64']=fnv(WORK/'benchmark-snapshots'/record['snapshot'])
assert sha(WORK/'examples/diagnostics_cancellation_cost.rs')==sha(EVIDENCE/'diagnostics_cancellation_cost.rs')

environment=os.environ.copy()
build_env={'CARGO_TARGET_DIR':str(TARGET),'CARGO_INCREMENTAL':'0','CARGO_BUILD_JOBS':'2',
    'NODES_COST_LIBRARY_SHA':CAPTURE['stable_library_revision'],
    'NODES_COST_BEFORE_SHA':SOURCES['before']['revision'],'NODES_COST_AFTER_SHA':SOURCES['after']['revision'],
    'NODES_COST_BEFORE_SHA256':SOURCES['before']['sha256'],'NODES_COST_AFTER_SHA256':SOURCES['after']['sha256']}
environment.update(build_env)
manifest={
    'experiment':'Cancellation instrumentation cost: unchanged diagnostic modules and three API paths',
    'started_at_utc':now(),'library_revision':CAPTURE['stable_library_revision'],
    'worktree':str(WORK),'target_directory':str(TARGET),
    'target_policy':'Exclusively owned existing dependency target; cargo clean -p morphlet --release forces fresh project library and example while retaining third-party dependencies. Both diagnostic implementations then coexist in that single fresh library.',
    'snapshots':list(SOURCES.values()),'harness_sha256':sha(WORK/'examples/diagnostics_cancellation_cost.rs'),
    'library_shim_sha256':sha(EVIDENCE/'library-module-shim.rs'),'full_lib_rs_sha256':sha(WORK/'src/lib.rs'),
    'cargo_toml_sha256':sha(WORK/'Cargo.toml'),'cargo_lock_sha256':sha(WORK/'Cargo.lock'),
    'worktree_status':command_output(['git','status','--porcelain']),
    'compiler':command_output([RUSTC,'-vV']),'cargo':command_output([CARGO,'-V']),
    'build_environment':build_env,'rustflags':environment.get('RUSTFLAGS',''),
    'profile':{'opt_level':3,'lto':'thin','codegen_units':16,'strip':True},
    'commands':[],'free_disk_bytes_before':shutil.disk_usage(ROOT).free,
}
def save(): (EVIDENCE/'build-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
save()
for label,command in [('package-clean',[CARGO,'clean','-p','morphlet','--release','--target-dir',str(TARGET)]),
                      ('paired-build',[CARGO,'build','--release','--locked','--offline','--example','diagnostics_cancellation_cost'])]:
    print(f'{label}: starting',flush=True)
    started=time.monotonic(); start_utc=now(); log=EVIDENCE/(label+'.log')
    with log.open('w') as stream:
        result=subprocess.run(command,cwd=WORK,env=environment,stdout=stream,stderr=subprocess.STDOUT)
    record={'label':label,'command':command,'started_at_utc':start_utc,'completed_at_utc':now(),'elapsed_seconds':time.monotonic()-started,'exit_code':result.returncode,'log':log.name}
    manifest['commands'].append(record); save()
    print(json.dumps(record),flush=True)
    if result.returncode:
        print(log.read_text()[-6000:],flush=True)
        raise SystemExit(result.returncode)

binary=ROOT/'diagnostics-benchmark-binaries/diagnostics-cancellation-cost'
shutil.copy2(TARGET/'release/examples/diagnostics_cancellation_cost',binary)
libraries=list((TARGET/'release').rglob('libmorphlet*.rlib'))
assert len(libraries)==1, libraries
library=libraries[0]
manifest.update({'completed_at_utc':now(),'binary':str(binary),'binary_sha256':sha(binary),'binary_size_bytes':binary.stat().st_size,
    'linked_library':str(library),'linked_library_sha256':sha(library),'library_mtime_ns':library.stat().st_mtime_ns,
    'free_disk_bytes_after':shutil.disk_usage(ROOT).free})
save()
print(json.dumps({'ready':True,'binary':str(binary),'sha256':manifest['binary_sha256'],'free_disk_bytes':manifest['free_disk_bytes_after']}),flush=True)
