import gzip
import io
import json
import sys
import time
from pathlib import Path
from common import python_model, sha

engine, path, output, repeats = sys.argv[1:]
data=Path(path).read_bytes()
if engine=='python-pptx':
    from pptx import Presentation
    def parse():return python_model(Presentation(io.BytesIO(data)))
else:
    from markitdown.converters import PptxConverter
    from markitdown import StreamInfo
    converter=PptxConverter()
    def parse():return {'markdown':converter.convert(io.BytesIO(data),StreamInfo(extension='.pptx')).markdown}

try:
    start=time.perf_counter();first=parse();cold=(time.perf_counter()-start)*1000
    loops=min(10,max(1,int(10/max(.01,cold))+1));samples=[]
    for _ in range(int(repeats)):
        elapsed=0
        for _ in range(loops):
            start=time.perf_counter();result=parse();elapsed+=(time.perf_counter()-start)*1000
            del result
        samples.append(elapsed/loops)
    if output!='-':
        for s in first.get('slides',[]):s['images']=[sha(x) if isinstance(x,bytes) else x for x in s['images']]
        with gzip.open(output,'wt') as f:json.dump(first,f)
    print(json.dumps(dict(status='ok',cold_ms=cold,loops=loops,samples_ms=samples)))
except Exception as exc:
    print(json.dumps(dict(status='error',error=type(exc).__name__+': '+str(exc))))
