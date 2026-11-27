from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,os,platform,statistics,subprocess,time

ROOT=Path('/workspace/scratch/74d3da15ab33')
OUT=ROOT/'NODES/docs/development/diagnostics-cancellation-cost'
BUILD=json.loads((OUT/'build-manifest.json').read_text())
BINARY=Path(BUILD['binary'])
CASES=['success-10000','prefix-1024','interleaved-512']
PATHS=['before','after_wrapper','after_atomic']

def now(): return datetime.now(timezone.utc).isoformat()
def sha(path):
    with path.open('rb') as stream: return hashlib.file_digest(stream,'sha256').hexdigest()
def validate(raw,case,reverse):
    assert raw['status']=='MEASURED' and raw['reverse_order']==reverse and len(raw['cases'])==1
    assert raw['provenance']['distinct_linked_wrapper_function_addresses']
    for source in BUILD['snapshots']:
        label=source['label'];p=raw['provenance']
        assert p[label+'_diagnostics_sha256']==source['sha256']
        assert p[label+'_embedded_snapshot_fnv1a64']==source['fnv1a64']
        assert p[label+'_embedded_snapshot_bytes']==source['bytes']
    result=raw['cases'][0]
    assert result['name']==case
    assert result['oracle']['independently_verified_implementations']==sorted(PATHS)
    assert result['oracle']['all_calls_equal_complete_serialized_report']
    for path in PATHS: assert len(result['implementations'][path]['repeated']['samples_ms'])==5
    assert len(result['rounds'])==6
    for index,round_record in enumerate(result['rounds']):
        rotated=PATHS[index%3:]+PATHS[:index%3]
        expected=list(reversed(rotated)) if reverse else rotated
        assert [item['implementation'] for item in round_record['measurements']]==expected
    return result

assert not (OUT/'measurement-manifest.json').exists(), 'Do not overwrite prior evidence'
assert sha(BINARY)==BUILD['binary_sha256']
models=sorted({line.partition(':')[2].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')})
mem={line.partition(':')[0]:line.partition(':')[2].strip() for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith(('MemTotal:','MemAvailable:'))}
manifest={'started_at_utc':now(),'binary':str(BINARY),'binary_sha256':BUILD['binary_sha256'],
    'protocol':'Two processes per case; each evaluates once, then records one first-report call plus five repeated calls for all three paths. Path order rotates per round; each corresponding round is reversed exactly in the second process. Case order reverses too. Ten repeated samples per path/case; first calls excluded. All complete report bytes must match the independently verified oracle within each process.',
    'quiet_window':'Parent authorized measurement after owned build/test processes stopped; shared host load is not guaranteed idle.',
    'host':{'system':platform.system(),'release':platform.release(),'machine':platform.machine(),'available_logical_cpus':len(os.sched_getaffinity(0)),'cpu_models':models,'memory_at_start':mem},
    'resource_scope':'CPU and peak RSS describe the entire three-path process, including graph/evaluation, all three implementations, serialization and parsed oracle checks; no per-path memory claim.',
    'processes':[]}
def save(): (OUT/'measurement-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
save();collected={case:{'paths':{path:[] for path in PATHS},'records':[]} for case in CASES}
for process_round,order,reverse in [(1,CASES,False),(2,list(reversed(CASES)),True)]:
    for case in order:
        assert sha(BINARY)==BUILD['binary_sha256']
        name=f'{case}-round-{process_round}'
        command=[str(BINARY),'--case',case,'--repetitions','5']
        if reverse: command.append('--reverse')
        print(f'Starting {name}; reverse={reverse}',flush=True)
        started=time.monotonic();start_utc=now()
        with (OUT/(name+'.json')).open('w') as out,(OUT/(name+'.stderr.log')).open('w') as err:
            child=subprocess.Popen(command,stdout=out,stderr=err)
            _,status,usage=os.wait4(child.pid,0)
            exit_code=os.waitstatus_to_exitcode(status);child.returncode=exit_code
        record={'name':name,'case':case,'process_round':process_round,'reverse':reverse,'command':command,
            'started_at_utc':start_utc,'completed_at_utc':now(),'wall_seconds':time.monotonic()-started,'exit_code':exit_code,
            'user_cpu_seconds':usage.ru_utime,'system_cpu_seconds':usage.ru_stime,'maximum_rss_kib':usage.ru_maxrss,
            'raw_json':name+'.json','stderr_log':name+'.stderr.log'}
        manifest['processes'].append(record);save()
        if exit_code: raise SystemExit(f'{name} failed with exit{exit_code}')
        result=validate(json.loads((OUT/record['raw_json']).read_text()),case,reverse)
        record['source_oracle_and_order_verified']=True
        for path in PATHS: collected[case]['paths'][path].extend(result['implementations'][path]['repeated']['samples_ms'])
        collected[case]['records'].append(result);save()
        print(json.dumps({'case':case,'round':process_round,'median_ms':{path:result['implementations'][path]['repeated']['median_ms'] for path in PATHS}}),flush=True)
manifest['completed_at_utc']=now();manifest['all_exit_codes_zero']=True;manifest['all_source_oracle_and_order_checks_passed']=True;save()
summary={'started_at_utc':manifest['started_at_utc'],'completed_at_utc':manifest['completed_at_utc'],
    'pooling':'Median of ten repeated samples/path/case pooled from two processes with reversed corresponding orders. First-report calls excluded. Positive percentage change means slower diagnosis.',
    'cases':[]}
for case in CASES:
    records=collected[case]['records'];paths={}
    for path in PATHS:
        samples=collected[case]['paths'][path];assert len(samples)==10
        paths[path]={'samples_ms':samples,'median_ms':statistics.median(samples),'min_ms':min(samples),'max_ms':max(samples),
            'first_report_calls_ms':[row['implementations'][path]['first_report_call_ms'] for row in records],
            'first_serialization_ms':[row['implementations'][path]['first_report_serialization_ms'] for row in records]}
    before=paths['before']['median_ms'];wrapper=paths['after_wrapper']['median_ms'];atomic=paths['after_atomic']['median_ms']
    summary['cases'].append({'case':case,'nodes':records[0]['nodes'],'expected_roots':records[0]['expected_roots'],'expected_blocked':records[0]['expected_blocked'],
        'paths':paths,'after_wrapper_change_vs_before_percent':(wrapper/before-1)*100,'after_atomic_change_vs_before_percent':(atomic/before-1)*100,
        'after_atomic_change_vs_wrapper_percent':(atomic/wrapper-1)*100,
        'fixture_build_ms':[row['fixture_build_and_validate_ms'] for row in records],'evaluation_ms':[row['real_evaluation_wall_ms'] for row in records],
        'json_bytes':[row['oracle']['full_report_json_bytes'] for row in records],'oracle':records[0]['oracle']})
(OUT/'comparison-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({'finished':True,'cases':[{key:row[key] for key in ['case','after_wrapper_change_vs_before_percent','after_atomic_change_vs_before_percent','after_atomic_change_vs_wrapper_percent']} for row in summary['cases']]}),flush=True)
