import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import urllib.request

from feedback import OUT, ROOT


def main():
    runtime = OUT / 'poi-runtime'
    runtime.mkdir(exist_ok=True)
    executable = str(Path(os.environ['JAVA_HOME']) / 'bin/java') if os.environ.get('JAVA_HOME') else shutil.which('java')
    if not executable:
        raise SystemExit('Java 17 or newer is required.')
    settings = subprocess.run([executable, '-XshowSettings:properties', '-version'], capture_output=True, text=True, check=True).stderr
    java_home = Path(re.search(r'^\s*java.home = (.+)$', settings, re.M).group(1).strip())
    version = re.search(r'^\s*java.specification.version = (.+)$', settings, re.M).group(1).strip()
    if int(version) < 17:
        raise SystemExit('Java 17 or newer is required.')
    url = 'https://repo.maven.apache.org/maven2/org/apache/maven/apache-maven/3.9.11/apache-maven-3.9.11-bin.tar.gz'
    archive = runtime / url.rsplit('/', 1)[-1]
    if not archive.exists():
        archive.write_bytes(urllib.request.urlopen(url, timeout=60).read())
    expected = urllib.request.urlopen(url + '.sha512', timeout=30).read().decode().split()[0]
    actual = hashlib.sha512(archive.read_bytes()).hexdigest()
    assert actual == expected
    with tarfile.open(archive) as source:
        source.extractall(runtime, filter='data')
    (runtime / 'maven-download.json').write_text(json.dumps(dict(url=url, sha512=actual, bytes=archive.stat().st_size), indent=2))
    pom = ROOT / 'scripts/pptx-benchmark/poi/pom.xml'
    command = [str(runtime / 'apache-maven-3.9.11/bin/mvn'), '-B', '-q', '-f', str(pom), '-Dmaven.repo.local=' + str(runtime / 'repository'), 'org.apache.maven.plugins:maven-dependency-plugin:3.8.1:build-classpath', '-Dmdep.outputFile=' + str(runtime / 'classpath.txt')]
    subprocess.run(command, env=dict(os.environ, JAVA_HOME=str(java_home)), check=True)
    classpath = (runtime / 'classpath.txt').read_text().strip()
    source = ROOT / 'scripts/pptx-benchmark/poi/PowerPointReference.java'
    (runtime / 'classes').mkdir(exist_ok=True)
    subprocess.run([str(java_home / 'bin/javac'), '-cp', classpath, '-d', str(runtime / 'classes'), str(source)], check=True)
    jars = [Path(p) for p in classpath.split(os.pathsep)]
    identity = dict(poi='6.0.0', java_executable=str(java_home / 'bin/java'), java=subprocess.run([str(java_home / 'bin/java'), '-version'], capture_output=True, text=True, check=True).stderr, dependencies=[dict(path=str(p), sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in jars], source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), pom_sha256=hashlib.sha256(pom.read_bytes()).hexdigest(), class_sha256=hashlib.sha256((runtime / 'classes/PowerPointReference.class').read_bytes()).hexdigest(), dispatch='explicit HSLF for OLE and XMLSlideShow otherwise')
    (runtime / 'identity.json').write_text(json.dumps(identity, indent=2))
    print('Compiled Apache POI 6.0.0 reference with', len(jars), 'recorded dependency hashes.')


if __name__ == '__main__':
    main()
